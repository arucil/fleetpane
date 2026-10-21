use super::*;

#[derive(Default)]
pub(super) struct State {
    pub catalog: fleetpane_session::mcp::Catalog,
    pub observed: bool,
    pub selected: Option<String>,
    pub copy: Option<String>,
    pub request_host: Option<Uuid>,
    expanded: bool,
}

impl HostView {
    pub(super) fn render_mcp(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let blocked = self.busy || self.pending.is_some() || !self.can_act() || self.draining();
        let selected = self
            .mcp
            .selected
            .as_ref()
            .and_then(|name| self.mcp.catalog.servers.iter().find(|s| &s.name == name));
        div().flex().flex_col().flex_shrink_0().px_4().py_2().border_t_1().border_color(cx.theme().border)
            .child(div().flex().items_center().gap_2()
                .child(Button::new("toggle-mcp").small().ghost().label(if self.mcp.expanded {"Hide MCP servers"} else {"MCP servers"})
                    .on_click(cx.listener(|this, _, _, cx| {this.mcp.expanded = !this.mcp.expanded;cx.notify();})))
                .when(self.mcp.observed, |header| header.child(div().text_xs().child(format!("{} loaded{}", self.mcp.catalog.servers.len(), if self.mcp.catalog.complete {""} else {" · incomplete"}))))
            )
            .when(self.mcp.expanded, |pane| pane
                .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Host configuration inventory. Discovery may start or connect configured MCP servers. It does not establish an active session connection."))
                .child(div().flex().items_center().flex_wrap().gap_2().py_2()
                    .child(Button::new("discover-mcp").small().label("Discover servers").disabled(blocked)
                        .on_click(cx.listener(|this, _, window, cx| this.command(Command::Mcp {cursor:None}, window, cx))))
                    .when_some(self.mcp.catalog.next_cursor.clone(), |row, cursor| row.child(
                        Button::new("more-mcp").small().ghost().label("Load more servers").disabled(blocked)
                            .on_click(cx.listener(move |this, _, window, cx| this.command(Command::Mcp {cursor:Some(cursor.clone())}, window, cx)))
                    ))
                    .when(self.mcp.copy.is_some(), |row| {
                        let owner = cx.entity().downgrade();
                        row.child(Button::new("copy-mcp").small().ghost().label("Copy inventory")
                            .on_click(move |_, _, cx| {
                                if let Some(owner) = owner.upgrade()
                                    && let Some(raw) = owner.read(cx).mcp.copy.clone()
                                { cx.write_to_clipboard(ClipboardItem::new_string(raw)); }
                            }))
                    })
                    .when(!self.mcp.catalog.servers.is_empty(), |row| row.child(
                        Button::new("select-mcp-server").small().ghost().label(self.mcp.selected.clone().unwrap_or_else(|| "Choose server".into()))
                            .dropdown_menu_with_anchor(Anchor::BottomLeft, {let owner=cx.entity().downgrade();move |menu, _, cx| {
                                let Some(view)=owner.upgrade() else {return menu;};
                                let view=view.read(cx);
                                let mut menu=menu.scrollable(true).max_h(px(320.));
                                for server in &view.mcp.catalog.servers {
                                    let name=server.name.clone();
                                    let target=owner.clone();
                                    menu=menu.item(PopupMenuItem::new(name.clone()).checked(view.mcp.selected.as_ref()==Some(&name))
                                        .on_click(move |_, _, cx| {
                                            if let Some(view)=target.upgrade() {
                                                view.update(cx, |view, cx| {view.mcp.selected=Some(name.clone());cx.notify();});
                                            }
                                        }));
                                }
                                menu
                            }})
                    ))
                )
                .when(!self.mcp.observed, |pane| pane.child(div().text_xs().child("No inventory loaded. Discover explicitly to inspect provider configuration.")))
                .when(self.mcp.observed && self.mcp.catalog.servers.is_empty(), |pane| pane.child(div().text_xs().child("Provider returned no configured servers.")))
                .when_some(selected, |pane, server| {
                    let tool_names = server.tools.iter().take(32).map(|(key, tool)| if key == &tool.name {key.clone()} else {format!("{key} ({})",tool.name)}).collect::<Vec<_>>().join(" · ");
                    pane.child(div().text_xs().child(format!("{} · Auth: {} · Runtime: {} · {} tools",server.name,server.auth_status,server.runtime_status.as_deref().unwrap_or("Unavailable"),server.tools.len())))
                        .when_some(server.tools_error.clone(), |pane, error| pane.child(div().text_xs().text_color(cx.theme().danger).child(format!("Tool discovery: {error}"))))
                        .child(div().text_xs().child(tool_names))
                        .when(server.tools.len()>32, |pane| pane.child(div().text_xs().child("Showing the first 32 tool names. Copy inventory includes all loaded tools and schemas.")))
                })
            )
    }
}
