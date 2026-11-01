//! Metadata-aware recall and conservative complete renderings; Core selects everything.
use super::*;
use respire_app::review::{AuthorizedRecallSource, ReviewService};

pub fn require_metadata(brief: bool, min_score: Option<f32>) -> Result<()> {
    super::require_render_budget(min_score)?;
    let caps: Value = respire::core_sdk::execute("capabilities", Value::Null)?;
    anyhow::ensure!(
        caps["memory_metadata_contract"] == 1,
        "Core SDK does not support memory_metadata_contract 1; metadata recall refused"
    );
    if brief {
        anyhow::ensure!(
            caps["brief_render_contract"] == 1,
            "Core SDK does not support brief_render_contract 1; brief recall refused"
        );
    }
    if respire_app::read_heat::effective()?.is_some() {
        anyhow::ensure!(
            caps["read_heat_contract"] == 1,
            "Core SDK does not support read_heat_contract 1; explicit read_heat refused"
        );
    }
    Ok(())
}

pub fn brief_layout(limit: usize) -> Result<RenderBudget> {
    anyhow::ensure!(limit <= 200, "brief recall limit must be at most 200");
    let mut prefixes = vec!["【respire · brief】\n".to_owned(); limit + 1];
    prefixes[0] = String::new();
    Ok(RenderBudget {
        budget_chars: 1500,
        prefix_by_count: prefixes,
        separator: "\n\n".to_owned(),
        suffix: String::new(),
        items: Vec::new(),
    })
}

#[derive(Deserialize)]
struct MetadataResult {
    items: Vec<RecalledWithContext>,
    mode: String,
    warning: Option<String>,
    routing: respire::memory::model::RecallRouting,
    retrieval: Value,
    budget: Option<Value>,
    filtered_by_score: Option<usize>,
    intent: String,
    procedure_pass: usize,
    procedure_promoted: bool,
    evidence: Vec<Value>,
    filtered_by_evidence: usize,
}

fn full_text(entry: &respire::MemoryEntry) -> Result<String> {
    let mut text = format!(
        "- {}（{}; kind={}）\n{}",
        entry.title,
        entry.id,
        entry.kind.as_str(),
        entry.content
    );
    text.push_str(&format!("\nRecorded source: {}",serde_json::to_string(&json!({"user":entry.user,"project":entry.project,"computer":entry.computer,"device":entry.device,"created_at":entry.created_at}))?));
    if let Some(summary) = &entry.metadata.summary {
        text.push_str(&format!("\nStored summary: {summary}"));
    }
    if !entry.metadata.source_refs.is_empty() {
        text.push_str(&format!(
            "\nSources: {}",
            serde_json::to_string(&entry.metadata.source_refs)?
        ));
    }
    if let Some(artifact) = &entry.metadata.artifact {
        text.push_str(&format!("\nArtifact: {}", serde_json::to_string(artifact)?));
    }
    Ok(text)
}

fn brief_request(
    layout: &RenderBudget,
    source: &AuthorizedRecallSource,
    evaluations: &[respire_app::review::Evaluation],
) -> Result<Value> {
    anyhow::ensure!(
        layout.budget_chars <= 1500,
        "brief dynamic block exceeds the 1500 character contract"
    );
    let mut items = Vec::new();
    for entry in source.entries() {
        let evaluation = evaluations
            .iter()
            .find(|item| item.id == entry.id)
            .context("Core omitted render source evaluation")?;
        let Some(revision) = &evaluation.record_revision else {
            continue;
        };
        // Full includes every original body/semantic field; no shorter variant
        // is qualified merely because a summary/title happens to exist.
        items.push(json!({"id":entry.id,"record_revision":revision,"representation":"full","complete":true,"rendered_text":full_text(entry)?}));
        if let Some(summary) = &entry.metadata.summary {
            items.push(json!({"id":entry.id,"record_revision":revision,"representation":"summary","complete":false,"rendered_text":format!("- {}（{}）\n{}",entry.title,entry.id,summary)}));
        }
        items.push(json!({"id":entry.id,"record_revision":revision,"representation":"title","complete":false,"rendered_text":format!("- {}（{}）",entry.title,entry.id)}));
    }
    Ok(
        json!({"mode":"brief","budget_chars":layout.budget_chars,"prefix_by_count":layout.prefix_by_count,
        "separator":layout.separator,"suffix":layout.suffix,"items":items}),
    )
}

fn validate_receipt(
    receipt: &Value,
    request: &Value,
    results: &[RecalledWithContext],
    evidence: &[Value],
) -> Result<()> {
    let items = receipt["items"]
        .as_array()
        .context("missing brief receipt items")?;
    let text = receipt["rendered_text"]
        .as_str()
        .context("missing brief rendered_text")?;
    let budget = request["budget_chars"]
        .as_u64()
        .context("missing brief budget")?;
    let chars = text.chars().count();
    anyhow::ensure!(
        receipt["chars"].as_u64() == Some(u64::try_from(chars)?)
            && chars <= 1500
            && u64::try_from(chars)? <= budget
            && receipt["budget_chars"].as_u64() == Some(budget)
            && receipt["selected_count"].as_u64() == Some(u64::try_from(items.len())?)
            && receipt.get("tokens").is_some_and(Value::is_null)
            && receipt.get("tokenizer_id").is_some_and(Value::is_null)
            && receipt["estimated"] == true,
        "brief budget count/token receipt mismatch; block refused"
    );
    let status = receipt["status"].as_str().context("missing brief status")?;
    anyhow::ensure!(
        matches!(
            status,
            "selected"
                | "empty_candidates"
                | "all_dropped_by_budget"
                | "all_dropped_by_incomplete"
                | "all_dropped"
                | "wrapper_over_budget"
        ),
        "unknown brief budget status"
    );
    anyhow::ensure!(
        (status == "selected") == !items.is_empty(),
        "brief selected status/item mismatch"
    );
    let mut ids = std::collections::HashSet::new();
    let mut bodies = Vec::new();
    for item in items {
        let id = item["id"].as_str().context("missing brief selected id")?;
        anyhow::ensure!(ids.insert(id), "duplicate selected brief id");
        let supplied = request["items"]
            .as_array()
            .context("missing brief request items")?
            .iter()
            .find(|source| {
                source["id"] == id
                    && source["representation"] == item["representation"]
                    && source["complete"] == true
            })
            .context("Core selected an unqualified/unknown brief variant")?;
        let body = item["rendered_text"]
            .as_str()
            .context("missing brief item text")?;
        anyhow::ensure!(
            body.ends_with(
                supplied["rendered_text"]
                    .as_str()
                    .context("missing original rendering")?
            ),
            "Core changed complete source rendering"
        );
        let checked = evidence
            .iter()
            .find(|value| value["id"] == id)
            .context("missing selected evidence")?;
        anyhow::ensure!(
            item["evidence"] == *checked
                && item["evidence"]["record_revision"] == supplied["record_revision"],
            "brief receipt evidence/revision mismatch"
        );
        bodies.push(body);
    }
    anyhow::ensure!(
        results
            .iter()
            .map(|item| item.entry.id.as_str())
            .eq(items.iter().filter_map(|item| item["id"].as_str())),
        "brief result/receipt order mismatch"
    );
    if status == "wrapper_over_budget" {
        anyhow::ensure!(
            items.is_empty() && text.is_empty(),
            "over-budget brief wrapper returned text"
        );
    } else {
        let prefix = request["prefix_by_count"]
            .get(items.len())
            .and_then(Value::as_str)
            .context("brief prefix coverage mismatch")?;
        let joined = bodies.join(request["separator"].as_str().context("missing separator")?);
        anyhow::ensure!(
            text == format!(
                "{}{}{}",
                prefix,
                joined,
                request["suffix"].as_str().context("missing suffix")?
            ),
            "brief wrapper rendering mismatch"
        );
    }
    Ok(())
}

pub fn recall_metadata<E: respire::memory::search::Embedder>(
    service: &ReviewService<'_>,
    source: &AuthorizedRecallSource,
    embedder: &E,
    query: &respire::MemoryQuery,
    store: &respire::transport::local::LocalStore,
    layout: Option<RenderBudget>,
    min_score: Option<f32>,
    intent: &str,
    mode: Option<&str>,
) -> Result<(
    Vec<RecalledWithContext>,
    String,
    Option<String>,
    respire::memory::model::RecallRouting,
    Value,
    Option<Value>,
)> {
    let mode = mode.map(str::to_owned).unwrap_or_else(|| {
        respire::service::read_agent_config()["recall_mode"]
            .as_str()
            .unwrap_or("fast")
            .to_owned()
    });
    anyhow::ensure!(
        matches!(mode.as_str(), "fast" | "quality") && matches!(intent, "auto" | "fact" | "howto"),
        "unknown recall mode/intent"
    );
    let settings = respire::feedback::query_settings(store, query, source.memories())?;
    let now = settings["now"]
        .as_str()
        .context("SDK query_settings omitted now")?;
    let scope = respire::memory::model::Scope {
        project: query.project.clone(),
        device: Some(respire::service::device_tag()),
        version: None,
        from: None,
        until: None,
        unknown_fields: Default::default(),
    };
    let (context, evaluations, authority_errors) = service.recall_evidence(source, &scope, now)?;
    let budget = layout
        .as_ref()
        .map(|layout| brief_request(layout, source, &evaluations))
        .transpose()?;
    let provider = if mode == "quality" {
        super::configured_provider()
    } else {
        None
    };
    let mut transport = |request: &Value| -> Result<Value> {
        let backend = provider
            .as_ref()
            .context("model provider is not configured")?;
        crate::classify::call_api(backend, request)
    };
    let ids: Vec<String> = source
        .memories()
        .iter()
        .filter(|memory| !memory.deleted)
        .map(|memory| memory.id.clone())
        .collect();
    let runs: std::collections::HashMap<String, Value> = store
        .skill_run_stats(&ids)?
        .into_iter()
        .map(|(id, stats)| (id, json!({"ok":stats.ok,"fail":stats.fail})))
        .collect();
    let mut payload = json!({"model":embedder.model_name(),"snapshots":source.snapshots(),"query":query,"mode":mode,
        "query_settings":settings,"evidence_context":context,"intent":intent,"skill_runs":runs,
        "provider":provider.as_ref().map(|backend|json!({"name":backend.name,"model":backend.model}))});
    if let Some(budget) = &budget {
        payload["render_budget"] = budget.clone();
    }
    if let Some(score) = min_score {
        payload["render_min_score"] = json!(score);
    }
    let mut result: MetadataResult =
        respire::core_sdk::execute_with_transport("query_business", payload, &mut transport)?;
    anyhow::ensure!(
        matches!(result.intent.as_str(), "typed" | "fact" | "howto"),
        "invalid Core intent receipt"
    );
    if min_score.is_some() {
        anyhow::ensure!(
            result.filtered_by_score.is_some(),
            "missing score filter receipt"
        );
    }
    if let Some(request) = budget {
        let receipt = result
            .budget
            .as_ref()
            .context("Core omitted brief budget receipt")?;
        validate_receipt(receipt, &request, &result.items, &result.evidence)?;
    }
    for evidence in &result.evidence {
        let id = evidence["id"].as_str().context("missing evidence id")?;
        let original = evaluations
            .iter()
            .find(|item| item.id == id)
            .context("unexpected Core evidence id")?;
        anyhow::ensure!(
            *evidence == serde_json::to_value(original)?,
            "query evidence differs from same-snapshot evaluation"
        );
    }
    anyhow::ensure!(
        result.items.iter().all(|item| result
            .evidence
            .iter()
            .any(|evidence| evidence["id"] == item.entry.id)),
        "Core omitted ranked item evidence"
    );
    result.retrieval["intent"] = json!(result.intent);
    result.retrieval["procedure_pass"] = json!(result.procedure_pass);
    result.retrieval["procedure_promoted"] = json!(result.procedure_promoted);
    result.retrieval["filtered_by_score"] = json!(result.filtered_by_score);
    result.retrieval["evidence"] = json!(result.evidence);
    result.retrieval["filtered_by_evidence"] = json!(result.filtered_by_evidence);
    // `code` is a credential-reserved output key outside review commands.
    result.retrieval["authority_errors"] = json!(authority_errors.iter().map(|error|
        json!({"id":error["id"],"reason":error["code"]})).collect::<Vec<_>>());
    result.retrieval["memory_revision"] = json!(source.memory_revision());
    result.retrieval["superseded"] = json!({});
    result.retrieval["representation_qualification"] =
        json!({"full":"complete original body and semantic fields","summary":false,"title":false});
    Ok((
        result.items,
        result.mode,
        result.warning,
        result.routing,
        result.retrieval,
        result.budget,
    ))
}
