//! The TendKit application: window, menus, and application-level actions.

use gpui_kit::{component::TitleBar, *};
use tendkit_model::AppModel;
use tendkit_storage::WindowSize;
use tendkit_ui::{Assets, Hide, HideOthers, Minimize, Quit, ScanAgain, ShowAll, Workspace, Zoom, TOOLBAR_HEIGHT};

/// Shared handles for application-level actions such as Quit.
struct AppState {
  model: Entity<AppModel>,
  window: Option<AnyWindowHandle>,
}

impl Global for AppState {}

fn main() {
  gpui_kit::application().with_assets(Assets).run(|cx| {
    gpui_kit::init(cx);
    cx.activate(true);
    bind_keys(cx);

    let model = cx.new(|_| AppModel::new());
    cx.set_global(AppState { model: model.clone(), window: None });
    cx.on_action(|_: &Quit, cx| request_quit(cx));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &ScanAgain, cx| {
      let model = cx.global::<AppState>().model.clone();
      model.update(cx, |model, cx| model.refresh(cx));
    });
    // TendKit has a single window, so closing it quits.
    cx.on_window_closed(|cx, _| {
      if cx.windows().is_empty() {
        cx.quit();
      }
    })
    .detach();

    let saved_size = WindowSize::load(&WindowSize::default_path());
    let bounds = Bounds::centered(None, size(px(saved_size.width), px(saved_size.height)), cx);
    let options = WindowOptions {
      window_bounds: Some(WindowBounds::Windowed(bounds)),
      window_min_size: Some(size(px(WindowSize::MIN.width), px(WindowSize::MIN.height))),
      app_id: Some("com.cagatayldzz.tendkit".into()),
      // Center the traffic lights in the shared toolbar.
      titlebar: Some(TitlebarOptions { traffic_light_position: Some(point(px(18.), TOOLBAR_HEIGHT / 2. - px(6.))), ..TitleBar::title_bar_options() }),
      ..TitleBar::window_options()
    };
    let window = gpui_kit::open_window(options, cx, |window, cx| {
      window.on_window_should_close(cx, |window, cx| {
        let should_close = confirm_close(window, cx);
        if should_close {
          save_window_size(window);
        }
        should_close
      });
      cx.new(|cx| Workspace::new(model.clone(), window, cx))
    })
    .expect("Failed to open the TendKit window");
    cx.global_mut::<AppState>().window = Some(window.0);
    cx.on_app_quit(|cx| {
      if let Some(window) = cx.global::<AppState>().window {
        let _ = window.update(cx, |_, window, _| save_window_size(window));
      }
      async {}
    })
    .detach();
    model.update(cx, |model, cx| model.scan(cx));
  });
}

fn save_window_size(window: &Window) {
  let size = window.window_bounds().get_bounds().size;
  let saved_size = WindowSize { width: size.width.into(), height: size.height.into() };
  if let Err(error) = saved_size.save(&WindowSize::default_path()) {
    eprintln!("Could not save window size: {error}");
  }
}

fn bind_keys(cx: &mut App) {
  cx.bind_keys([
    KeyBinding::new("cmd-q", Quit, None),
    KeyBinding::new("cmd-h", Hide, None),
    KeyBinding::new("alt-cmd-h", HideOthers, None),
    KeyBinding::new("cmd-r", ScanAgain, None),
    KeyBinding::new("cmd-m", Minimize, None),
  ]);
  cx.on_action(|_: &Minimize, cx| {
    if let Some(window) = cx.active_window() {
      let _ = window.update(cx, |_, window, _| window.minimize_window());
    }
  });
  cx.on_action(|_: &Zoom, cx| {
    if let Some(window) = cx.active_window() {
      let _ = window.update(cx, |_, window, _| window.zoom_window());
    }
  });
  cx.set_menus([
    Menu::new("TendKit").items([
      MenuItem::action("Hide TendKit", Hide),
      MenuItem::action("Hide Others", HideOthers),
      MenuItem::action("Show All", ShowAll),
      MenuItem::separator(),
      MenuItem::action("Quit TendKit", Quit),
    ]),
    Menu::new("Edit").items([
      MenuItem::os_action("Undo", gpui_kit::component::input::Undo, OsAction::Undo),
      MenuItem::os_action("Redo", gpui_kit::component::input::Redo, OsAction::Redo),
      MenuItem::separator(),
      MenuItem::os_action("Cut", gpui_kit::component::input::Cut, OsAction::Cut),
      MenuItem::os_action("Copy", gpui_kit::component::input::Copy, OsAction::Copy),
      MenuItem::os_action("Paste", gpui_kit::component::input::Paste, OsAction::Paste),
      MenuItem::os_action("Select All", gpui_kit::component::input::SelectAll, OsAction::SelectAll),
    ]),
    Menu::new("View").items([MenuItem::action("Scan Again", ScanAgain)]),
    Menu::new("Window").items([MenuItem::action("Minimize", Minimize), MenuItem::action("Zoom", Zoom)]),
  ]);
}

/// Quitting during an installation asks first, then cancels the operation and
/// quits once the engine has stopped.
fn request_quit(cx: &mut App) {
  let state = cx.global::<AppState>();
  let (model, window) = (state.model.clone(), state.window);
  if !model.read(cx).is_executing {
    cx.quit();
    return;
  }
  if let Some(window) = window {
    let _ = window.update(cx, |_, window, cx| {
      confirm_close(window, cx);
    });
  }
}

/// Returns whether the window may close now. While an installation runs it
/// shows a confirmation and closes later, after cancellation finishes.
fn confirm_close(window: &mut Window, cx: &mut App) -> bool {
  let model = cx.global::<AppState>().model.clone();
  if !model.read(cx).is_executing {
    return true;
  }
  let answer = window.prompt(
    PromptLevel::Warning,
    "An installation is in progress",
    Some("Quitting cancels the operation. Completed installations are preserved. Close any open macOS Installer window separately."),
    &["Continue Operation", "Cancel and Quit"],
    cx,
  );
  cx.spawn(async move |cx| {
    if answer.await == Ok(1) {
      model.update(cx, |model, cx| {
        model.pending_termination = true;
        model.cancel(cx);
      });
    }
  })
  .detach();
  false
}
