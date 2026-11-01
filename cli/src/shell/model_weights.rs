//! Draft-only model picker. All writes/downloads go through existing commands.
use super::{choice, line, t, KeyCode, Line, Value};
use respire::memory::onnx::MODEL_VARIANTS;

const ENGINES: &[&str] = &["cpu", "gpu", "npu"];
pub(super) enum Action { Save, Install, Back }
pub(super) struct Form { pub cursor: usize, engine: usize, variant: usize }
impl Form {
    pub(super) fn new(settings: &Value) -> Self {
        Self { cursor: 0,
            engine: ENGINES.iter().position(|e| Some(*e) == settings["engine"].as_str()).unwrap_or(0),
            variant: MODEL_VARIANTS.iter().position(|v| Some(v.name) == settings["model"].as_str()).unwrap_or(0) }
    }
    pub(super) fn selection(&self) -> (&'static str, &'static str) {
        (ENGINES[self.engine], MODEL_VARIANTS[self.variant].name)
    }
    pub(super) fn key(&mut self, key: KeyCode) -> Option<Action> {
        let count = MODEL_VARIANTS.len() + 4;
        match key {
            KeyCode::Esc | KeyCode::Char('0') => return Some(Action::Back),
            KeyCode::Up => self.cursor = (self.cursor + count - 1) % count,
            KeyCode::Down => self.cursor = (self.cursor + 1) % count,
            KeyCode::Left if self.cursor == 0 => self.engine = (self.engine + ENGINES.len() - 1) % ENGINES.len(),
            KeyCode::Right if self.cursor == 0 => self.engine = (self.engine + 1) % ENGINES.len(),
            KeyCode::Enter if self.cursor == 0 => self.engine = (self.engine + 1) % ENGINES.len(),
            KeyCode::Enter if self.cursor <= MODEL_VARIANTS.len() => self.variant = self.cursor - 1,
            KeyCode::Enter if self.cursor == MODEL_VARIANTS.len() + 1 => return Some(Action::Save),
            KeyCode::Enter if self.cursor == MODEL_VARIANTS.len() + 2 => return Some(Action::Install),
            KeyCode::Enter => return Some(Action::Back),
            _ => {}
        }
        None
    }
    pub(super) fn present(settings: &Value, variant: &str) -> bool {
        settings["variants"].as_array().is_some_and(|rows| rows.iter()
            .any(|row| row["name"] == variant && row["state"] == "present"))
    }
    pub(super) fn lines(&self, settings: &Value) -> Vec<Line<'static>> {
        let (engine, _) = self.selection();
        let supported = super::model_engines().contains(&engine);
        let mut rows = vec![line(t("引擎与模型权重", "Engine and model weights")),
            line(format!("{}: {} / {}", t("已保存", "Saved"),
                settings["engine"].as_str().unwrap_or("unknown"), settings["model"].as_str().unwrap_or("unknown"))),
            choice(self.cursor == 0, format!("{}: {engine}  {}{}", t("草稿引擎", "Draft engine"),
                t("← → 选择", "← → choose"), if supported { String::new() }
                else { t("（此系统不支持）", " (unsupported on this OS)") }))];
        for (index, variant) in MODEL_VARIANTS.iter().enumerate() {
            let state = if Self::present(settings, variant.name) {
                t("文件存在，未验证推理", "present; inference unverified")
            } else { t("需安装 / 校验", "install / verify required") };
            rows.push(choice(self.cursor == index + 1, format!("{} {:<10} {:>4.0} MiB  {state}",
                if index == self.variant { "●" } else { "○" }, variant.name, variant.size as f64 / 1048576.0)));
        }
        rows.push(choice(self.cursor == MODEL_VARIANTS.len() + 1, t("保存引擎与所选权重", "Save engine and selected weights")));
        rows.push(choice(self.cursor == MODEL_VARIANTS.len() + 2, t("安装 / 校验所选权重…", "Install / verify selected weights…")));
        rows.push(choice(self.cursor == MODEL_VARIANTS.len() + 3, t("返回（放弃草稿）", "Back (discard draft)")));
        rows.push(line(t("上下移动，回车选权重；只选不保存不会改变配置。切引擎保留所选权重。",
            "Up/down moves; Enter picks weights. Drafts do not change settings. Engine changes keep the selected weights.")));
        rows.push(line(t("GPU/NPU 需要实际硬件及 provider；部分变体可能不支持。保存后在模型页运行推理验证。",
            "GPU/NPU need hardware/providers; some variants may fail. After saving, run Check inference on the Models page.")));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drafts_preserve_weights_on_engine_changes_and_cancel_without_an_action() {
        let settings = serde_json::json!({"engine":"cpu","model":"fp16"});
        let mut form = Form::new(&settings);
        form.key(KeyCode::Right);
        assert_eq!(form.selection(), ("gpu", "fp16"));
        assert!(matches!(form.key(KeyCode::Esc), Some(Action::Back)));
        assert_eq!(settings["engine"], "cpu");
        assert_eq!(Form::new(&settings).selection(), ("cpu", "fp16"));
    }
    #[test]
    fn all_variants_are_selectable_without_saving_or_downloading() {
        let mut form = Form::new(&serde_json::json!({"engine":"npu","model":"quantized"}));
        for (index, variant) in MODEL_VARIANTS.iter().enumerate() {
            form.cursor = index + 1;
            assert!(form.key(KeyCode::Enter).is_none());
            assert_eq!(form.selection(), ("npu", variant.name));
        }
        assert!(!Form::present(&serde_json::json!({"variants":[{"name":"q4f16","state":"invalid_size"}]}), "q4f16"));
    }
}
