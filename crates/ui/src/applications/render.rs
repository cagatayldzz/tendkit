//! The applications page layout: filters, the list, and row actions.

use super::*;

impl Render for ApplicationsPage {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if !cfg!(target_os = "macos") {
      return div().p_5().child(t("apps.unsupported")).into_any_element();
    }
    let theme = cx.theme().clone();
    let busy = self.scanning || self.removing.is_some();
    let query = self.search.read(cx).value().trim().to_lowercase();
    let protected = self.inventory.applications.iter().filter(|app| app.blocked.is_some()).count();
    let removable = self.inventory.applications.len() - protected;
    let mut apps: Vec<_> = self
      .inventory
      .applications
      .iter()
      .filter(|app| {
        let matches_filter = match self.filter {
          Filter::All => true,
          Filter::Removable => app.blocked.is_none(),
          Filter::Protected => app.blocked.is_some(),
          Filter::Running => self.running.contains(&app.path),
        };
        matches_filter
          && (app.name.to_lowercase().contains(&query)
            || app.path.to_string_lossy().to_lowercase().contains(&query)
            || app.bundle_id.as_ref().is_some_and(|id| id.to_lowercase().contains(&query)))
      })
      .collect();
    if self.sort_size {
      apps.sort_by(|left, right| right.bytes.cmp(&left.bytes).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase())));
    }
    let shown = apps.len();
    let mut filters = h_flex().gap_1().flex_wrap();
    for (filter, key, count) in [
      (Filter::Removable, "apps.removable", removable),
      (Filter::All, "apps.all", self.inventory.applications.len()),
      (Filter::Protected, "apps.protected", protected),
      (Filter::Running, "apps.running", self.inventory.applications.iter().filter(|app| self.running.contains(&app.path)).count()),
    ] {
      filters = filters.child(
        Button::new(SharedString::from(key))
          .ghost()
          .small()
          .selected(self.filter == filter)
          .label(tf("apps.filter_count", &[("name", &t(key)), ("count", &count.to_string())]))
          .on_click(cx.listener(move |this, _, _, cx| {
            this.filter = filter;
            this.visible = 100;
            cx.notify();
          })),
      );
    }
    let mut list = v_flex().w_full();
    for (index, app) in apps.iter().take(self.visible).enumerate() {
      let is_running = self.running.contains(&app.path);
      let usage = self.usage.get(&app.path);
      let cpu = usage.and_then(|usage| usage.cpu).map(|cpu| format!("{cpu:.1}%")).unwrap_or_else(|| "—".into());
      let memory = usage.filter(|usage| usage.memory > 0).map(|usage| size(usage.memory)).unwrap_or_else(|| "—".into());
      let removing = self.removing.as_ref() == Some(&app.path);
      let expanded = self.expanded.as_ref() == Some(&app.path);
      let toggle_path = app.path.clone();
      let context_path = app.path.clone();
      let reveal = app.path.clone();
      let open = app.path.clone();
      let copy = app.path.clone();
      let app_to_remove = (*app).clone();
      let reason = app.blocked.map(block_key);
      let origin = t(match app.origin {
        ApplicationOrigin::System => "apps.origin_system",
        ApplicationOrigin::Shared => "apps.origin_shared",
        ApplicationOrigin::User => "apps.origin_user",
      });
      let mut details = v_flex()
        .gap_3()
        .mx_5()
        .mb_3()
        .p_3()
        .rounded_md()
        .bg(theme.foreground.opacity(0.035))
        .child(div().text_xs().child(app.path.to_string_lossy().into_owned()))
        .when_some(app.bundle_id.clone(), |column, id| column.child(div().text_xs().text_color(theme.muted_foreground).child(id)));
      if let Some(reason) = reason {
        details = details.child(div().text_sm().child(t(reason)));
      } else if is_running {
        details = details.child(div().text_sm().child(t("apps.running_reason")));
      }
      details = details.child(
        h_flex()
          .gap_2()
          .flex_wrap()
          .child(
            Button::new(SharedString::from(format!("app-open-{index}")))
              .ghost()
              .small()
              .icon(Lucide::AppWindow)
              .label(t("apps.open_app"))
              .on_click(move |_, _, cx| cx.open_with_system(&open)),
          )
          .child(
            Button::new(SharedString::from(format!("app-reveal-{index}")))
              .ghost()
              .small()
              .icon(Lucide::Folder)
              .label(t("disk.reveal"))
              .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
          )
          .child(
            Button::new(SharedString::from(format!("app-copy-{index}")))
              .ghost()
              .small()
              .icon(Lucide::Copy)
              .label(t("disk.copy_path"))
              .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string_lossy().into_owned()))),
          )
          .when(reason.is_none(), |row| {
            row.child(
              Button::new(SharedString::from(format!("app-remove-detail-{index}")))
                .ghost()
                .small()
                .icon(Lucide::Trash)
                .label(t(if removing { "apps.moving" } else { "apps.remove" }))
                .disabled(busy || is_running || self.model.read(cx).busy())
                .on_click(cx.listener(move |this, _, window, cx| this.confirm_removal(app_to_remove.clone(), window, cx))),
            )
          })
          .when(app.blocked == Some(RemovalBlock::System), |row| {
            row.child(
              Button::new(SharedString::from(format!("app-help-{index}")))
                .ghost()
                .small()
                .label(t("apps.help"))
                .on_click(|_, _, cx| cx.open_url("https://support.apple.com/102610")),
            )
          }),
      );
      let app_to_remove = (*app).clone();
      let row = h_flex()
        .id(SharedString::from(format!("application-{index}")))
        .w_full()
        .min_h(px(64.))
        .gap_3()
        .px_5()
        .py_2()
        .cursor_pointer()
        .hover(|style| style.bg(theme.foreground.opacity(0.035)))
        .on_click(cx.listener(move |this, _, _, cx| {
          this.expanded = if this.expanded.as_ref() == Some(&toggle_path) { None } else { Some(toggle_path.clone()) };
          cx.notify();
        }))
        .child(Icon::new(Lucide::AppWindow).size(px(24.)).text_color(if app.blocked.is_some() { theme.muted_foreground } else { theme.blue }))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(
              h_flex()
                .gap_2()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(app.name.clone()))
                .when_some(app.version.clone(), |row, version| row.child(div().text_xs().text_color(theme.muted_foreground).child(version))),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(origin)),
        )
        .when(app.blocked.is_some(), |row| row.child(Tag::secondary().small().child(t("apps.protected"))))
        .when(is_running, |row| row.child(Tag::info().small().child(t("apps.running"))))
        .child(div().w(px(72.)).text_right().text_sm().child(cpu))
        .child(div().w(px(94.)).text_right().text_sm().child(memory))
        .child(div().w(px(90.)).text_right().text_sm().child(app.bytes.map(size).unwrap_or_else(|| "—".into())))
        .child(
          Button::new(SharedString::from(format!("application-action-{index}")))
            .ghost()
            .small()
            .icon(if reason.is_some() { Lucide::ShieldCheck } else { Lucide::Trash })
            .tooltip(t(reason.unwrap_or(if is_running { "apps.running_reason" } else { "apps.remove_tip" })))
            .disabled(reason.is_none() && (busy || is_running || self.model.read(cx).busy()))
            .on_click(cx.listener(move |this, _, window, cx| {
              if app_to_remove.blocked.is_some() {
                this.expanded = Some(app_to_remove.path.clone());
                cx.notify();
              } else {
                this.confirm_removal(app_to_remove.clone(), window, cx);
              }
            })),
        )
        .child(Icon::new(if expanded { IconName::ChevronDown } else { IconName::ChevronRight }).small())
        .context_menu(move |menu, _, _| {
          let open = context_path.clone();
          let reveal = context_path.clone();
          let copy = context_path.clone();
          menu
            .item(PopupMenuItem::new(t("apps.open_app")).icon(Lucide::AppWindow).on_click(move |_, _, cx| cx.open_with_system(&open)))
            .item(PopupMenuItem::new(t("disk.reveal")).icon(Lucide::Folder).on_click(move |_, _, cx| cx.reveal_path(&reveal)))
            .item(
              PopupMenuItem::new(t("disk.copy_path"))
                .icon(Lucide::Copy)
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string_lossy().into_owned()))),
            )
        });
      list = list.child(v_flex().w_full().border_b_1().border_color(theme.border).child(row).when(expanded, |column| column.child(details)));
    }
    if !self.scanning && apps.is_empty() {
      list =
        list.child(v_flex().p_6().gap_2().items_center().text_color(theme.muted_foreground).child(t("apps.empty"))).when(!query.is_empty(), |list| {
          list.child(div().flex().justify_center().pb_4().child(
            Button::new("apps-clear-search").ghost().small().label(t("apps.clear_search")).on_click(cx.listener(|this, _, window, cx| {
              this.search.update(cx, |search, cx| search.set_value("", window, cx));
            })),
          ))
        });
    }
    if !self.scanning && apps.is_empty() && self.filter != Filter::All {
      list = list.child(div().flex().justify_center().pb_4().child(
        Button::new("apps-search-all").ghost().small().label(t("apps.search_all")).on_click(cx.listener(|this, _, _, cx| {
          this.filter = Filter::All;
          cx.notify();
        })),
      ));
    }
    if shown > self.visible {
      list = list.child(div().p_3().child(Button::new("applications-more").ghost().label(t("disk.more")).on_click(cx.listener(|this, _, _, cx| {
        this.visible += 100;
        cx.notify();
      }))));
    }
    v_flex()
      .size_full()
      .child(
        v_flex()
          .w_full()
          .gap_3()
          .px_5()
          .py_3()
          .border_b_1()
          .border_color(theme.border)
          .child(
            h_flex()
              .gap_2()
              .child(
                div()
                  .flex_1()
                  .min_w_0()
                  .child(Input::new(&self.search).with_size(component::Size::Medium).prefix(Icon::new(IconName::Search).small()).cleanable(true)),
              )
              .child(
                Button::new("applications-sort")
                  .ghost()
                  .small()
                  .icon(if self.sort_size { Lucide::ArrowDownWideNarrow } else { Lucide::ArrowDownAZ })
                  .tooltip(t(if self.sort_size { "disk.sort_size" } else { "disk.sort_name" }))
                  .on_click(cx.listener(|this, _, _, cx| {
                    this.sort_size = !this.sort_size;
                    this.visible = 100;
                    cx.notify();
                  })),
              )
              .child(
                Button::new("applications-refresh")
                  .ghost()
                  .small()
                  .icon(IconName::RefreshCw)
                  .tooltip(t("apps.refresh"))
                  .disabled(busy)
                  .on_click(cx.listener(|this, _, _, cx| this.scan(cx))),
              )
              .child(
                Button::new("applications-info")
                  .ghost()
                  .small()
                  .icon(IconName::Info)
                  .selected(self.scope_expanded)
                  .tooltip(t("apps.about"))
                  .on_click(cx.listener(|this, _, _, cx| {
                    this.scope_expanded = !this.scope_expanded;
                    cx.notify();
                  })),
              ),
          )
          .child(
            h_flex()
              .gap_2()
              .flex_wrap()
              .child(filters)
              .child(div().flex_1())
              .child(div().text_xs().text_color(theme.muted_foreground).child(tf("apps.visible_count", &[("count", &shown.to_string())]))),
          )
          .when(self.scope_expanded, |view| {
            view.child(
              v_flex()
                .gap_2()
                .p_3()
                .rounded_md()
                .bg(theme.foreground.opacity(0.035))
                .child(div().text_xs().text_color(theme.muted_foreground).child(t("apps.scope")))
                .child(div().text_xs().child(t("apps.protection_note")))
                .child(div().text_xs().child(t("apps.usage_note")))
                .child(
                  Button::new("applications-help")
                    .ghost()
                    .small()
                    .label(t("apps.help"))
                    .on_click(|_, _, cx| cx.open_url("https://support.apple.com/102610")),
                ),
            )
          })
          .when(self.inventory.incomplete, |view| view.child(div().text_xs().child(t("apps.incomplete"))))
          .when(self.scanning, |view| view.child(div().text_sm().child(t("apps.scanning"))))
          .when_some(self.error.clone(), |view, error| view.child(div().text_sm().text_color(theme.danger).child(error)))
          .when_some(self.message.clone(), |view, message| view.child(div().text_sm().child(message))),
      )
      .when_some(self.usage_error.clone(), |view, error| {
        view.child(div().px_5().py_2().text_xs().child(tf("apps.usage_unavailable", &[("error", &error)])))
      })
      .child(
        h_flex()
          .px_5()
          .py_2()
          .gap_3()
          .text_xs()
          .text_color(theme.muted_foreground)
          .child(div().flex_1().child(t("apps.application")))
          .child(div().w(px(72.)).text_right().child(t("apps.cpu")))
          .child(div().w(px(94.)).text_right().child(t("apps.ram")))
          .child(div().w(px(90.)).text_right().child(t("disk.size")))
          .child(div().w(px(60.))),
      )
      .child(div().id("applications-scroll").flex_1().min_h_0().overflow_y_scroll().child(list))
      .into_any_element()
  }
}
