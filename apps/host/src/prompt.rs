//! CLI input boundaries: parse first, then read at most one exact UTF-8 source.
use fleetpane_session::{MAX_PROMPT_BYTES, client::Error};
use std::{fs::File, io::Read, path::PathBuf};

#[derive(Debug, PartialEq, Eq)]
enum Source {
    Text(String),
    File(PathBuf),
    Stdin,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Input {
    source: Source,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl Input {
    pub fn parse(args: &[String], options: bool) -> Result<Self, Error> {
        let mut source = None;
        let mut model = None;
        let mut effort = None;
        let mut rest = args;
        while let Some((flag, tail)) = rest.split_first() {
            rest = tail;
            let next = match flag.as_str() {
                "--stdin" => Source::Stdin,
                "--prompt" | "--prompt-file" => {
                    let (value, tail) = rest
                        .split_first()
                        .ok_or_else(|| Error::Input(format!("{flag} requires a value")))?;
                    rest = tail;
                    if flag == "--prompt" {
                        Source::Text(value.clone())
                    } else {
                        if value.is_empty() {
                            return Err(Error::Input("--prompt-file requires a path".into()));
                        }
                        Source::File(value.into())
                    }
                }
                "--model" | "--effort" if options => {
                    let (value, tail) = rest
                        .split_first()
                        .ok_or_else(|| Error::Input(format!("{flag} requires a value")))?;
                    rest = tail;
                    let slot = if flag == "--model" {
                        &mut model
                    } else {
                        &mut effort
                    };
                    if slot.is_some() || value.trim().is_empty() {
                        return Err(Error::Input(format!("{flag} is duplicate or empty")));
                    }
                    *slot = Some(value.clone());
                    continue;
                }
                "--" => {
                    let [text] = rest else {
                        return Err(Error::Input(
                            "-- must be followed by one literal prompt argument".into(),
                        ));
                    };
                    rest = &[];
                    Source::Text(text.clone())
                }
                value if value.starts_with('-') => {
                    return Err(Error::Input(format!(
                        "Unknown input flag {value}; use -- before a literal prompt starting with '-'"
                    )));
                }
                _ => Source::Text(flag.clone()),
            };
            if source.replace(next).is_some() {
                return Err(Error::Input(
                    "Provide exactly one of TEXT, --prompt TEXT, --prompt-file PATH or --stdin"
                        .into(),
                ));
            }
        }
        let source = source.ok_or_else(|| {
            Error::Input(
                "Missing prompt: choose TEXT, --prompt TEXT, --prompt-file PATH or --stdin".into(),
            )
        })?;
        if effort.is_some() && model.is_none() {
            return Err(Error::Input("--effort requires --model".into()));
        }
        Ok(Self {
            source,
            model,
            effort,
        })
    }

    pub fn read(&self, stdin: &mut impl Read) -> Result<String, Error> {
        let bytes = match &self.source {
            Source::Text(text) => text.as_bytes().to_vec(),
            Source::Stdin => read_bounded(stdin)
                .map_err(|error| Error::Input(format!("Prompt stdin: {error}")))?,
            Source::File(path) => {
                // Open without waiting for a FIFO peer, then validate the actual
                // opened object, rather than racing a separate path stat.
                let descriptor = rustix::fs::open(
                    path.as_path(),
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::NONBLOCK
                        | rustix::fs::OFlags::NOCTTY
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
                .map_err(|error| {
                    Error::Input(format!("Prompt file {}: {error}", path.display()))
                })?;
                let file = File::from(descriptor);
                if !file
                    .metadata()
                    .map_err(|error| Error::Input(format!("Prompt file metadata: {error}")))?
                    .is_file()
                {
                    return Err(Error::Input("Prompt file must be a regular file".into()));
                }
                read_bounded(file).map_err(|error| {
                    Error::Input(format!("Prompt file {}: {error}", path.display()))
                })?
            }
        };
        if bytes.len() > MAX_PROMPT_BYTES {
            return Err(Error::Input(
                "Prompt exceeds 64 KiB; no Host was contacted".into(),
            ));
        }
        let text = String::from_utf8(bytes)
            .map_err(|error| Error::Input(format!("Prompt is not UTF-8: {error}")))?;
        if text.trim().is_empty() {
            return Err(Error::Input("Prompt requires non-whitespace text".into()));
        }
        Ok(text)
    }
}

fn read_bounded(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_PROMPT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(args: &[&str], options: bool) -> Result<Input, Error> {
        Input::parse(
            &args.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
            options,
        )
    }

    #[test]
    fn sources_preserve_exact_text_and_options_never_supply_another_prompt() {
        let text = "  中文\r\ntrailing  \n";
        for args in [vec![text], vec!["--prompt", text], vec!["--stdin"]] {
            assert_eq!(
                parse(&args, true)
                    .unwrap()
                    .read(&mut Cursor::new(text))
                    .unwrap(),
                text
            );
        }
        let input = parse(
            &["--model", "model", "--prompt", text, "--effort", "high"],
            true,
        )
        .unwrap();
        assert_eq!(input.model.as_deref(), Some("model"));
        assert_eq!(input.effort.as_deref(), Some("high"));
        assert_eq!(input.read(&mut Cursor::new("unused")).unwrap(), text);
        assert_eq!(
            parse(&["--", "--literal"], false)
                .unwrap()
                .read(&mut Cursor::new(""))
                .unwrap(),
            "--literal"
        );
        for args in [
            vec![],
            vec!["one", "two"],
            vec!["--stdin", "--stdin"],
            vec!["--stdin", "--prompt-file", "unused"],
            vec!["--prompt"],
            vec!["--prompt-file", ""],
            vec!["--effort", "high", text],
            vec!["--stdin", "--model", "a", "--model", "b"],
            vec!["--unknown"],
            vec!["--", "a", "b"],
        ] {
            assert!(parse(&args, true).is_err(), "{args:?}");
        }
        assert!(parse(&["--stdin", "--model", "a"], false).is_err());
    }

    #[test]
    fn byte_limits_and_utf8_refusal_are_bounded_before_host_access() {
        let input = parse(&["--stdin"], false).unwrap();
        assert_eq!(
            input
                .read(&mut Cursor::new(vec![b'x'; MAX_PROMPT_BYTES]))
                .unwrap()
                .len(),
            MAX_PROMPT_BYTES
        );
        let mut stream = Cursor::new(vec![b'x'; MAX_PROMPT_BYTES * 2]);
        assert!(input.read(&mut stream).is_err());
        assert_eq!(stream.position(), (MAX_PROMPT_BYTES + 1) as u64);
        for bytes in [vec![0xff], b" \r\n\t".to_vec(), vec![]] {
            assert!(input.read(&mut Cursor::new(bytes)).is_err());
        }
    }
}
