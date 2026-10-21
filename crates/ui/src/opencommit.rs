use super::{
  components::orange,
  settings::{labeled, section},
  Workspace,
};
use crate::platform;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
  component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Textarea, TextareaState},
    select::{Select, SelectState},
    switch::Switch,
    v_flex, Icon, IconName, IndexPath, WindowExt,
  },
  *,
};
use tendkit_i18n::{t, tf};
use tendkit_storage::{shell_context, OpenCommitConfig};

type Choice = SelectState<Vec<SharedString>>;

/// Keeps an unrecognised existing value selectable instead of dropping it.
fn options<T: Clone + PartialEq>(supported: &[T], current: &T) -> Vec<T> {
  if supported.contains(current) {
    supported.to_vec()
  } else {
    std::iter::once(current.clone()).chain(supported.iter().cloned()).collect()
  }
}

fn token_label(limit: u32) -> SharedString {
  let label = tf("oc.tokens", &[("count", &platform::grouped(limit))]);
  if limit == OpenCommitConfig::RECOMMENDED_INPUT_TOKENS {
    tf("oc.recommended", &[("label", &label)]).into()
  } else if limit == OpenCommitConfig::OPENCOMMIT_DEFAULT_INPUT_TOKENS {
    tf("oc.opencommit_default", &[("label", &label)]).into()
  } else {
    label.into()
  }
}

/// Editable copy of the OpenCommit settings TendKit exposes.
pub struct OpenCommitSettings {
  config: OpenCommitConfig,
  models: Vec<String>,
  model: Entity<Choice>,
  token_limits: Vec<u32>,
  max_input_tokens: Entity<Choice>,
  languages: Vec<String>,
  language: Entity<Choice>,
  toggles: [(&'static str, &'static str, bool); 5],
  git_push: bool,
  instruction: Entity<TextareaState>,
  error: Option<String>,
}

impl OpenCommitSettings {
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let mut error = None;
    let config = OpenCommitConfig::load(&OpenCommitConfig::default_path()).unwrap_or_else(|e| {
      error = Some(tf("oc.read_error", &[("path", &OpenCommitConfig::default_path().display().to_string()), ("error", &e.to_string())]));
      OpenCommitConfig::default()
    });
    let instruction = shell_context::load(&shell_context::default_path()).unwrap_or_else(|e| {
      error = Some(e.to_string());
      None
    });

    let current_model = config.get("OCO_MODEL").unwrap_or_else(|| OpenCommitConfig::MODELS[0].into());
    let models = options(&OpenCommitConfig::MODELS.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &current_model);
    // OpenCommit's own default splits most real diffs, so treat it as unset.
    let tokens = config.get("OCO_TOKENS_MAX_INPUT").and_then(|v| v.parse().ok()).unwrap_or(OpenCommitConfig::OPENCOMMIT_DEFAULT_INPUT_TOKENS);
    let tokens = if tokens == OpenCommitConfig::OPENCOMMIT_DEFAULT_INPUT_TOKENS { OpenCommitConfig::RECOMMENDED_INPUT_TOKENS } else { tokens };
    let token_limits = options(OpenCommitConfig::INPUT_TOKEN_LIMITS, &tokens);
    let current_language = config.get("OCO_LANGUAGE").unwrap_or_else(|| "en".into());
    let languages = options(&OpenCommitConfig::LANGUAGES.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &current_language);

    let mut choice = |items: Vec<SharedString>, selected: usize| cx.new(|cx| SelectState::new(items, Some(IndexPath::new(selected)), window, cx));
    let model = choice(models.iter().map(|m| SharedString::from(m.clone())).collect(), models.iter().position(|m| *m == current_model).unwrap_or(0));
    let max_input_tokens =
      choice(token_limits.iter().map(|l| token_label(*l)).collect(), token_limits.iter().position(|l| *l == tokens).unwrap_or(0));
    let language = choice(
      languages.iter().map(|code| SharedString::from(platform::language_name(code))).collect(),
      languages.iter().position(|code| *code == current_language).unwrap_or(0),
    );
    let instruction_text = instruction.unwrap_or_else(|| shell_context::DEFAULT_INSTRUCTION.into());
    let instruction =
      cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 6).placeholder(t("oc.no_instructions")).default_value(instruction_text));
    let toggles = [
      ("OCO_ONE_LINE_COMMIT", "oc.one_line", config.bool("OCO_ONE_LINE_COMMIT", false)),
      ("OCO_DESCRIPTION", "oc.description", config.bool("OCO_DESCRIPTION", false)),
      ("OCO_WHY", "oc.why", config.bool("OCO_WHY", false)),
      ("OCO_EMOJI", "oc.gitmoji", config.bool("OCO_EMOJI", false)),
      ("OCO_OMIT_SCOPE", "oc.omit_scope", config.bool("OCO_OMIT_SCOPE", false)),
    ];
    let git_push = config.bool("OCO_GITPUSH", true);
    Self { config, models, model, token_limits, max_input_tokens, languages, language, toggles, git_push, instruction, error }
  }

  fn selected(state: &Entity<Choice>, cx: &App) -> usize {
    state.read(cx).selected_index(cx).map(|i| i.row).unwrap_or(0)
  }

  fn save(&mut self, cx: &mut Context<Self>) -> bool {
    let mut updated = self.config.clone();
    updated.set("OCO_MODEL", Some(&self.models[Self::selected(&self.model, cx)]));
    updated.set("OCO_TOKENS_MAX_INPUT", Some(&self.token_limits[Self::selected(&self.max_input_tokens, cx)].to_string()));
    updated.set("OCO_LANGUAGE", Some(&self.languages[Self::selected(&self.language, cx)]));
    updated.set("OCO_PROMPT_MODULE", Some(OpenCommitConfig::PROMPT_MODULE));
    for (key, _, value) in self.toggles {
      updated.set_bool(key, value);
    }
    updated.set_bool("OCO_GITPUSH", self.git_push);
    let instruction = self.instruction.read(cx).value().to_string();
    let result = updated.save(&OpenCommitConfig::default_path()).map_err(|e| e.to_string()).and_then(|_| {
      self.config = updated;
      shell_context::save(&instruction, &shell_context::default_path(), &shell_context::backups_path()).map_err(|e| e.to_string())
    });
    match result {
      Ok(()) => true,
      Err(error) => {
        self.error = Some(tf("oc.save_error", &[("error", &error)]));
        cx.notify();
        false
      }
    }
  }

  fn switch(&self, id: &'static str, label: String, checked: bool, cx: &mut Context<Self>, set: impl Fn(&mut Self, bool) + 'static) -> AnyElement {
    h_flex()
      .text_size(px(13.))
      .child(label)
      .child(div().flex_1())
      .child(Switch::new(id).checked(checked).on_click(cx.listener(move |this, checked: &bool, _, cx| {
        set(this, *checked);
        cx.notify();
      })))
      .into_any_element()
  }
}

impl Render for OpenCommitSettings {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let picker = |label: String, state: &Entity<Choice>| {
      h_flex()
        .gap_4()
        .text_size(px(13.))
        .child(label)
        .child(div().flex_1())
        .child(div().w(px(230.)).child(Select::new(state).menu_max_h(px(260.))))
        .into_any_element()
    };
    let generation = vec![
      picker(t("oc.model"), &self.model),
      picker(t("oc.max_diff"), &self.max_input_tokens),
      picker(t("oc.language"), &self.language),
      labeled(t("oc.convention"), "Conventional Commits", cx),
    ];
    let message = self
      .toggles
      .iter()
      .enumerate()
      .map(|(index, (key, label, value))| self.switch(key, t(label), *value, cx, move |this, checked| this.toggles[index].2 = checked))
      .collect();
    let git = vec![self.switch("OCO_GITPUSH", t("oc.push"), self.git_push, cx, |this, checked| this.git_push = checked)];
    let path = OpenCommitConfig::default_path();

    v_flex()
      .id("opencommit-settings")
      .max_h(px(470.))
      .overflow_y_scroll()
      .gap(px(18.))
      .child(section(t("oc.generation"), generation, Some(t("oc.generation_note").into()), cx))
      .child(section(t("oc.message"), message, None, cx))
      .child(section(t("oc.instructions"), vec![Textarea::new(&self.instruction).into_any_element()], Some(t("oc.instructions_note").into()), cx))
      .child(section(t("oc.git"), git, Some(tf("oc.git_note", &[("path", &path.display().to_string())]).into()), cx))
      .when_some(self.error.clone(), |this, error| {
        this.child(h_flex().gap_2().text_size(px(13.)).text_color(orange()).child(Icon::new(IconName::TriangleAlert).size(px(13.))).child(error))
      })
  }
}

impl Workspace {
  pub(super) fn open_opencommit_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let settings = cx.new(|cx| OpenCommitSettings::new(window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
      let save = settings.clone();
      dialog.w(px(480.)).title(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(t("oc.title"))).child(settings.clone()).footer(
        h_flex()
          .w_full()
          .child(Button::new("opencommit-cancel").label(t("progress.cancel")).on_click(|_, window, cx| window.close_dialog(cx)))
          .child(div().flex_1())
          .child(Button::new("opencommit-save").primary().label(t("action.save")).on_click(move |_, window, cx| {
            if save.update(cx, |settings, cx| settings.save(cx)) {
              window.close_dialog(cx);
            }
          })),
      )
    });
  }
}
