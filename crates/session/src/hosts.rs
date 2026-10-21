//! Named local Host endpoints. A name is a UI label, never a runtime identity.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

pub const MAX_HOSTS: usize = 8;
pub const MAX_CONFIG_BYTES: usize = 16 * 1024;

#[cfg(unix)]
pub mod diagnostics;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub name: String,
    pub socket: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub version: u32,
    pub hosts: Vec<Endpoint>,
    /// Explicit membership/priority for read-only new-session recommendations.
    /// Omitted configurations keep manual Host selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub name: String,
    pub hosts: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("Host configuration: {0}")]
pub struct Error(String);

impl Configuration {
    pub fn single(socket: PathBuf) -> Result<Self, Error> {
        let value = Self {
            version: 1,
            hosts: vec![Endpoint {
                name: "Local".into(),
                socket,
            }],
            routes: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(Error("file exceeds 16 KiB".into()));
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| Error(e.to_string()))?;
        value.validate()?;
        Ok(value)
    }

    /// Startup/CLI file I/O only; never called from rendering or polling.
    pub fn read(path: &Path) -> Result<Self, Error> {
        use std::io::Read;
        let file = std::fs::File::open(path)
            .map_err(|e| Error(format!("cannot open {}: {e}", path.display())))?;
        let mut bytes = Vec::new();
        file.take((MAX_CONFIG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| Error(format!("cannot read {}: {e}", path.display())))?;
        Self::parse(&bytes)
    }

    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.version != 1 {
            return Err(Error("unsupported version; expected 1".into()));
        }
        if self.hosts.is_empty() || self.hosts.len() > MAX_HOSTS {
            return Err(Error("choose between 1 and 8 Hosts".into()));
        }
        let mut names = BTreeSet::new();
        let mut sockets = BTreeSet::new();
        for host in &self.hosts {
            if host.name.is_empty()
                || host.name.trim() != host.name
                || host.name.chars().count() > 64
                || host.name.chars().any(char::is_control)
            {
                return Err(Error("names must contain 1–64 characters, without surrounding whitespace or control characters".into()));
            }
            if !names.insert(&host.name) {
                return Err(Error(format!("duplicate Host name: {}", host.name)));
            }
            if !host.socket.is_absolute()
                || host.socket.as_os_str().to_string_lossy().contains('\0')
                || !host
                    .socket
                    .components()
                    .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
                || host.socket.file_name().is_none()
            {
                return Err(Error(format!(
                    "{} requires an absolute socket path without parent-directory components",
                    host.name
                )));
            }
            if !sockets.insert(&host.socket) {
                return Err(Error("duplicate Host socket path".into()));
            }
        }
        if self.routes.len() > MAX_HOSTS {
            return Err(Error("choose at most 8 route groups".into()));
        }
        let mut routes = BTreeSet::new();
        for route in &self.routes {
            if route.name.is_empty()
                || route.name.trim() != route.name
                || route.name.chars().count() > 64
                || route.name.chars().any(char::is_control)
                || !routes.insert(&route.name)
            {
                return Err(Error("route names must be unique and contain 1–64 characters without surrounding whitespace or control characters".into()));
            }
            if route.hosts.is_empty() || route.hosts.len() > MAX_HOSTS {
                return Err(Error(
                    "route groups require 1–8 explicitly named Hosts".into(),
                ));
            }
            let mut members = BTreeSet::new();
            for name in &route.hosts {
                if !names.contains(name) || !members.insert(name) {
                    return Err(Error(format!(
                        "route {} has an unknown or repeated Host: {name}",
                        route.name
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
#[derive(Serialize)]
pub struct Probe {
    pub endpoint: Endpoint,
    #[serde(flatten)]
    pub result: ProbeResult,
}

#[cfg(unix)]
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProbeResult {
    Available {
        snapshot: Box<fleetpane_runtime::Snapshot>,
    },
    Unavailable {
        error: String,
    },
}

#[cfg(unix)]
pub async fn probe(configuration: Configuration) -> Result<Vec<Probe>, Error> {
    configuration.validate()?;
    let mut tasks = tokio::task::JoinSet::new();
    let count = configuration.hosts.len();
    for (index, endpoint) in configuration.hosts.into_iter().enumerate() {
        tasks.spawn(async move {
            let result = match crate::client::snapshot(&endpoint.socket).await {
                Ok(snapshot) => ProbeResult::Available {
                    snapshot: Box::new(snapshot),
                },
                Err(error) => ProbeResult::Unavailable {
                    error: error.to_string(),
                },
            };
            (index, Probe { endpoint, result })
        });
    }
    let mut reports = Vec::with_capacity(count);
    while let Some(result) = tasks.join_next().await {
        reports.push(result.map_err(|e| Error(format!("Host probe task failed: {e}")))?);
    }
    reports.sort_by_key(|(index, _)| *index);
    Ok(reports.into_iter().map(|(_, report)| report).collect())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_and_endpoints_are_independent_of_provider_thread_ids() {
        let bytes = serde_json::to_vec(&json!({"version":1,"hosts":[
            {"name":"OpenCodex","socket":"/tmp/first/control.sock"},
            {"name":"DeepSeek 开发","socket":"/tmp/second/control.sock"}
        ]}))
        .unwrap();
        let config = Configuration::parse(&bytes).unwrap();
        assert_eq!(config.hosts.len(), 2);
        assert_eq!(config.hosts[1].name, "DeepSeek 开发");
        assert_eq!(
            config.hosts[1].socket,
            PathBuf::from("/tmp/second/control.sock")
        );
        assert!(Configuration::single("/tmp/private/control.sock".into()).is_ok());
    }

    #[test]
    fn malformed_ambiguous_and_unbounded_configuration_fails() {
        let endpoint = json!({"name":"A","socket":"/tmp/a/control.sock"});
        for value in [
            json!({"version":2,"hosts":[endpoint.clone()]}),
            json!({"version":1,"hosts":[]}),
            json!({"version":1,"hosts":vec![endpoint.clone();MAX_HOSTS+1]}),
            json!({"version":1,"hosts":[endpoint.clone(),endpoint.clone()]}),
            json!({"version":1,"hosts":[endpoint.clone()],"token":"not accepted"}),
            json!({"version":1,"hosts":[{"name":"A","socket":"relative.sock"}]}),
            json!({"version":1,"hosts":[{"name":"A","socket":"/tmp/../a.sock"}]}),
            json!({"version":1,"hosts":[{"name":" A","socket":"/tmp/a.sock"}]}),
            json!({"version":1,"hosts":[{"name":"A\nB","socket":"/tmp/a.sock"}]}),
            json!({"version":1,"hosts":[{"name":"A","socket":"/tmp/a.sock","provider":"ignored?"}]}),
            json!({"version":1,"hosts":[endpoint.clone(),{"name":"B","socket":"/tmp/a/control.sock"}]}),
            json!({"version":1,"hosts":[endpoint,{"name":"A","socket":"/tmp/b/control.sock"}]}),
        ] {
            assert!(
                Configuration::parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
        }
        assert!(Configuration::parse(&vec![b' '; MAX_CONFIG_BYTES + 1]).is_err());
        assert!(Configuration::parse(b"{").is_err());
    }

    #[test]
    fn route_membership_is_explicit_ordered_and_validated() {
        let hosts = json!([
            {"name":"Old","socket":"/tmp/old/control.sock"},
            {"name":"New","socket":"/tmp/new/control.sock"}
        ]);
        let parse = |routes| {
            Configuration::parse(
                &serde_json::to_vec(&json!({"version":1,"hosts":hosts,"routes":routes})).unwrap(),
            )
        };
        let valid = parse(json!([{"name":"Codex","hosts":["New","Old"]}])).unwrap();
        assert_eq!(valid.routes[0].hosts, ["New", "Old"]);
        for routes in [
            json!([{"name":"Codex","hosts":[]}]),
            json!([{"name":"Codex","hosts":["missing"]}]),
            json!([{"name":"Codex","hosts":["New","New"]}]),
            json!([{"name":" Codex","hosts":["New"]}]),
            json!([{"name":"Codex","hosts":["New"],"fallback":"Old"}]),
            json!([{"name":"Codex","hosts":["New"]},{"name":"Codex","hosts":["Old"]}]),
            json!(vec![json!({"name":"Codex","hosts":["New"]}); MAX_HOSTS + 1]),
        ] {
            assert!(parse(routes.clone()).is_err(), "{routes}");
        }
        assert!(
            Configuration::single("/tmp/private/control.sock".into())
                .unwrap()
                .routes
                .is_empty()
        );
    }
}
