//! Procedural-memory helpers.
//!
//! Three responsibilities, all local to the CLI shell (no Core change):
//! 1. Skill-body template check (a skill entry is a manual, not an essay);
//! 2. How-to intent detection for recall routing;
//! 3. How-to ordering: merge a skill-only pass and keep one procedure visible.
//!
//! Run-history reads (see `skill_runs` in the local store) only break ties
//! between procedures. Fact entries keep Core's order and score.

use respire::memory::engine::RecalledWithContext;
use std::collections::HashMap;

/// Canonical labels a skill body should carry, inside the standard
/// `【前因】/【行为】/【后果】` markers (the markers stay mandatory).
pub const SECTIONS: [&str; 6] = ["触发", "前置", "步骤", "校验", "回退", "边界"];

/// Accepted spellings per section. Matching is substring-based so
/// `【触发条件】`, `触发条件：` and `Trigger:` all count.
const ALIASES: [(&str, &[&str]); 6] = [
    (
        "触发",
        &["触发条件", "触发", "何时用", "when to use", "trigger"],
    ),
    (
        "前置",
        &["前置条件", "前置", "前提", "prerequisite", "requires"],
    ),
    ("步骤", &["执行步骤", "步骤", "steps", "procedure"]),
    (
        "校验",
        &["成功判据", "校验", "验证", "verification", "verify"],
    ),
    (
        "回退",
        &[
            "失败回退",
            "失败处理",
            "回退",
            "兜底",
            "rollback",
            "fallback",
        ],
    ),
    (
        "边界",
        &["适用边界", "适用范围", "边界", "不适用", "scope", "limits"],
    ),
];

/// A label only counts as a section heading when it is followed by a separator
/// (`触发条件：`, `【步骤】`, `- 回退:`), so prose like "未验证" cannot stand in
/// for the 校验 section.
fn heading(lower: &str, alias: &str) -> bool {
    const SEPARATORS: [char; 11] = ['：', ':', '】', ']', ')', '-', '—', '、', '.', '。', '\n'];
    lower.match_indices(alias).any(|(at, _)| {
        let rest = lower[at + alias.len()..].trim_start();
        match rest.chars().next() {
            Some(c) => SEPARATORS.contains(&c),
            None => false,
        }
    })
}

/// Missing canonical sections. Empty means the body satisfies the template.
pub fn missing_sections(content: &str) -> Vec<&'static str> {
    let lower = content.to_lowercase();
    ALIASES
        .iter()
        .filter(|(_, needles)| !needles.iter().any(|n| heading(&lower, n)))
        .map(|(label, _)| *label)
        .collect()
}

/// Procedures are `skill` and `knowledge` entries; everything else is a fact.
pub fn is_procedure(kind: &str) -> bool {
    matches!(kind, "skill" | "knowledge")
}

/// Hit count from which a fact is worth re-reading during tidy (advisory only:
/// the AI still decides whether it is a procedure). Cumulative, because
/// `recall_count` carries no timestamps.
pub const HOT_RECALL_THRESHOLD: i64 = 5;

/// Loose operational smell: commands, paths, or numbered steps. Used only to
/// narrow the audit list; the upgrade decision stays with the AI.
pub fn looks_operational(content: &str) -> bool {
    const MARKERS: [&str; 14] = [
        "```", "$ ", "sudo ", "cd ", "cargo ", "npm ", "git ", "rsrs ", "python ", "node ", "步骤",
        "命令", "1. ", "1) ",
    ];
    let lower = content.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
        || content.contains('→')
        || content.contains('／')
        || content.contains("\\")
}

/// Conservative how-to detector. Only widens recall (adds a skill pass);
/// it never filters facts out, so a false positive costs one extra pass.
pub fn howto_intent(query: &str) -> bool {
    const NEEDLES: [&str; 26] = [
        "怎么", "如何", "怎样", "咋办", "报错", "失败", "错误", "步骤", "流程", "命令", "脚本",
        "配置", "安装", "部署", "调试", "排查", "修复", "跑通", "操作", "上传", "导入", "打包",
        "how to", "how do", "how can", "steps",
    ];
    let lower = query.to_lowercase();
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// Run history used for procedure ordering (ok/fail counts, 0/0 = unrun).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunScore {
    pub ok: i64,
    pub fail: i64,
}

impl RunScore {
    /// Small, bounded nudge: ±0.03 for all-ok / all-fail, 0 when unrun.
    fn delta(&self) -> f32 {
        let total = self.ok + self.fail;
        if total <= 0 {
            return 0.0;
        }
        0.03 * (self.ok - self.fail) as f32 / total as f32
    }
}

/// Merge a skill-only pass into the primary hits for a how-to query.
///
/// Union by id, order by score, then — if no procedure is already in the top
/// three and one exists — move the best-scored procedure to index 2.
/// "Best" ranks procedures by `score + run delta`, so a procedure that keeps
/// failing sinks below one that keeps working. Scores are never rewritten:
/// the caller still prints Core's own values, and `promoted` says whether the
/// order departs from them.
pub fn merge_howto(
    primary: Vec<RecalledWithContext>,
    secondary: Vec<RecalledWithContext>,
    limit: usize,
    runs: &HashMap<String, RunScore>,
) -> (Vec<RecalledWithContext>, bool) {
    let mut merged: Vec<RecalledWithContext> = primary;
    for row in secondary {
        if !merged.iter().any(|r| r.entry.id == row.entry.id) {
            merged.push(row);
        }
    }
    // Facts sort on Core's score; procedures get the bounded run nudge, so a
    // procedure that keeps failing sinks below one that keeps working.
    let procedure_rank = |r: &RecalledWithContext| {
        if !is_procedure(r.entry.kind.as_str()) {
            return r.score;
        }
        r.score
            + runs
                .get(&r.entry.id)
                .map(RunScore::delta)
                .unwrap_or_default()
    };
    merged.sort_by(|a, b| {
        procedure_rank(b)
            .partial_cmp(&procedure_rank(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.entry.id.cmp(&b.entry.id))
    });
    if merged.is_empty() {
        return (merged, false);
    }
    let best = merged
        .iter()
        .enumerate()
        .filter(|(_, r)| is_procedure(r.entry.kind.as_str()))
        .max_by(|(_, a), (_, b)| {
            procedure_rank(a)
                .partial_cmp(&procedure_rank(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx);
    let mut promoted = false;
    if let Some(idx) = best {
        if idx >= 3
            && merged[..3]
                .iter()
                .all(|r| !is_procedure(r.entry.kind.as_str()))
        {
            let row = merged.remove(idx);
            merged.insert(2, row);
            promoted = true;
        }
    }
    merged.truncate(limit);
    (merged, promoted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(id: &str, kind: &str, title: &str) -> respire::MemoryEntry {
        // Kind deserializes from its variant name (Context, Skill, ...), not the lowercase wire form.
        let variant = format!("{}{}", &kind[..1].to_uppercase(), &kind[1..]);
        serde_json::from_value(json!({
            "id": id, "kind": variant, "tags": [], "title": title, "content": "",
            "user": "tester", "computer": "test", "project": "",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "emotion": -1.0, "parent_id": "", "importance": "important"
        }))
        .expect("fixture entry")
    }

    fn row(id: &str, kind: &str, title: &str, score: f32) -> RecalledWithContext {
        RecalledWithContext {
            score,
            entry: entry(id, kind, title),
            ancestors: Vec::new(),
        }
    }

    #[test]
    fn template_accepts_aliases_and_flags_gaps() {
        let full = "【前因】x\n【行为】\n触发条件：不能本地跑\n前置：已登录\n步骤：1) run\n校验：退出码 0\n回退：还原备份\n边界：仅 Linux\n【后果】y";
        let missing = missing_sections(full);
        assert!(missing.is_empty(), "missing={missing:?}");

        let missing = missing_sections("【前因】x【行为】步骤：只写了一步【后果】y");
        assert_eq!(missing, vec!["触发", "前置", "校验", "回退", "边界"]);

        // Prose that merely contains an alias word does not satisfy the template.
        let prose = missing_sections("【前因】x【行为】步骤：结果未验证，边界情况很多【后果】y");
        assert!(prose.contains(&"校验"), "未验证 must not stand in for 校验");
    }

    #[test]
    fn intent_matches_procedural_questions_only() {
        assert!(howto_intent("OJ 上传题目报错怎么修"));
        assert!(howto_intent("how to rebuild the SDK"));
        assert!(howto_intent("部署失败排查"));
        assert!(!howto_intent("respire 品牌中文名"));
        assert!(!howto_intent("记忆系统是事实性还是程序性"));
    }

    #[test]
    fn merge_keeps_fact_order_but_surfaces_one_procedure() {
        let primary = vec![
            row("a", "context", "fact a", 0.9),
            row("b", "context", "fact b", 0.8),
            row("c", "context", "fact c", 0.7),
            row("d", "context", "fact d", 0.6),
        ];
        let secondary = vec![row("s", "skill", "procedure", 0.55)];
        let (merged, promoted) = merge_howto(primary, secondary, 4, &HashMap::new());
        assert!(promoted);
        let ids: Vec<&str> = merged.iter().map(|r| r.entry.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "s", "c"]);
        assert_eq!(merged[2].score, 0.55, "core score must stay untouched");
    }

    #[test]
    fn merge_does_not_displace_when_a_procedure_is_already_visible() {
        let primary = vec![
            row("s", "skill", "procedure", 0.9),
            row("a", "context", "fact a", 0.8),
            row("b", "context", "fact b", 0.7),
        ];
        let (merged, promoted) = merge_howto(primary, Vec::new(), 3, &HashMap::new());
        assert!(!promoted);
        assert_eq!(merged[0].entry.id, "s");
    }

    #[test]
    fn failing_procedure_sinks_below_working_one() {
        let primary = Vec::new();
        let secondary = vec![
            row("bad", "skill", "broken procedure", 0.70),
            row("good", "skill", "working procedure", 0.69),
        ];
        let mut runs = HashMap::new();
        runs.insert("bad".into(), RunScore { ok: 0, fail: 4 });
        runs.insert("good".into(), RunScore { ok: 4, fail: 0 });
        let (merged, promoted) = merge_howto(primary, secondary, 5, &runs);
        assert!(!promoted, "a procedure is already in the top three");
        let ids: Vec<&str> = merged.iter().map(|r| r.entry.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["good", "bad"],
            "run history reorders procedures only"
        );
        assert_eq!(merged[1].score, 0.70, "core score must stay untouched");
    }

    #[test]
    fn merge_dedupes_by_id() {
        let primary = vec![row("x", "skill", "same", 0.8)];
        let secondary = vec![row("x", "skill", "same", 0.9)];
        let (merged, _) = merge_howto(primary, secondary, 5, &HashMap::new());
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn operational_smell_separates_manuals_from_facts() {
        assert!(looks_operational(
            "步骤：1) cargo build -p respire；校验：退出码 0"
        ));
        assert!(looks_operational("修复方法 `git rebase --abort`"));
        assert!(!looks_operational("respire 的中文名是睿思记忆"));
        assert!(!looks_operational("这条记忆讲的是品牌命名结论"));
        assert_eq!(HOT_RECALL_THRESHOLD, 5);
    }

    #[test]
    fn empty_recall_stays_empty() {
        let (merged, promoted) = merge_howto(Vec::new(), Vec::new(), 10, &HashMap::new());
        assert!(merged.is_empty());
        assert!(!promoted);
    }

    #[test]
    fn knowledge_entries_are_procedures_too() {
        let primary = vec![
            row("a", "context", "fact a", 0.9),
            row("b", "context", "fact b", 0.8),
            row("c", "context", "fact c", 0.7),
        ];
        let secondary = vec![row("k", "knowledge", "manual", 0.5)];
        let (merged, promoted) = merge_howto(primary, secondary, 4, &HashMap::new());
        assert!(promoted);
        assert_eq!(merged[2].entry.id, "k");
        assert!(is_procedure("knowledge"));
        assert!(!is_procedure("task"));
    }
}
