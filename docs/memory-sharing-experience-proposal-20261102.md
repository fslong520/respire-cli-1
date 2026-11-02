# Memory sharing and experience exchange — design proposal (2026-11-02)

English summary: This proposal maps GitSwarm (arXiv:2610.04862) onto Respire's existing
`share` / `share-import` surface. The transferable idea is not "share text and agents get
stronger"; it is a provenance-carrying, status-aware import: keep the origin entry, separate
the structural parent from declared semantic dependencies, keep declared / observed / outcome
evidence apart, retain failed paths as scoped negative knowledge, and gate publication without
treating publication as correctness. The current implementation exports plaintext subtree text
without provenance, has no real merge, no idempotent import, no revocation, and no trust/status
fields. The MVP therefore fixes correctness and isolation first, then adds an experience-card
format and measurement. Status: design proposal, not implemented. Code references are against
the local `respire-cli` working tree at `fddfec6` (2026-11-02); upstream `main` may drift.
No experiment in section 10 has been run.

---

# GitSwarm 借鉴与 respire 分享记忆/经验设计（2026-11-02）

> 结论：值得借，但只借“来源、边、证据分层、负知识、发布闸门”，不借 Git 仓库、多 Agent 调度、投票与“祖先覆盖率=提升”。
> 当前 `share/share-import` 更像“导出文本包”，还不是“分享经验”。MVP 应先修正确性与安全，再谈经验卡片和效果度量。

## 0. 一句话结论

GitSwarm 真正有价值的不是“共享记忆让 Agent 变强”，而是它把推理过程变成**可持久、可分支、可归因的贡献图**，并把“声明用了什么 / 实际用了什么 / 最终结果继承了什么”分成三层证据。respire 应把“分享”从**复制文本**升级为**携带来源与状态的受控导入**，但不要把自己改成 Git 或多 Agent 运行时。

## 1. 文章机制与证据边界

文章对象：GitSwarm: Decentralized Compounding Inference（arXiv:2610.04862）。

可借机制：

1. **记忆=贡献账本**：一条记忆不只是正文，还带版本、父节点、分支、作者与“参考了谁”。
2. **实际父节点 vs 跨分支语义依赖分开**：`parent` 记录起手状态，`informed_by` 记录跨分支借用；后者是声明，不是因果。
3. **三层复用证据不可互换**：
   - 声明依赖（声称参考了什么）
   - 轨迹观察（实际打开/执行/改编了什么）
   - 最终结果祖先（最终答案继承了什么）
4. **失败与被否决路径也持久化**：失败结果回流，后续可引用、可比较、可否决。
5. **提交/提名/放弃三态**：只有通过发布检查的贡献才发布；提名只是偏好信号，不是正确性证明。
6. **分布式探索 + 中心化发布裁决**：贡献和探索可分散，调度、隔离、发布检查与最终选择仍集中。
7. **成本账**：历史越大，检查 token、重复工作、放弃开销越大；Agent 只选择性检查部分历史。
8. **独立验证边界**：发布检查 ≠ 正确；别的 Agent 说过 ≠ 事实；作者自认未做因果消融。

证据边界（不能写进 respire 承诺）：

- ProgramBench 79.4%、IMOProofBench 30/30 是特定子集、预算、模型、单次运行，部分对照不等算力。
- 82–93% 祖先覆盖率只属 ProgramBench；IMOProofBench 为 21.3–33.0%。
- 人工构造任务只证明“减少重复发现/遍历/追查成本”，不证明主评测得分提升。
- 调度比较仅 9 题、3 seed，作者明确不建立统计等价。
- 结论是相关性观察，不是“持久化记忆导致提升”的因果证明。

## 2. 对 respire 的借鉴映射

| 文章机制 | respire 现状 | 借法 | 判定 |
| --- | --- | --- | --- |
| 贡献账本 | 因果树 parent/child + `see_also`；share 包只有文本与父链 | 分享包带来源、边、状态、证据摘要 | 借，缩水 |
| parent vs informed_by | `parent` 是因果；`see_also` 对称；`source_refs` 是验证引用 | 不新造第四套边；先给 `see_also` 加方向/置信，或仅在密文内加单向 `informed_by` | 修订 |
| 三层证据 | `query-log`、`run-log`、`history` 各自存在 | 只做审计报表，不合成热度、不用于排序 | 借原则 |
| 失败留痕 | `run-log` 有 fail/no_effect/confounded | 负知识带 scope，默认不进正向召回，匹配场景作警告 | 借，改语义 |
| 三态闸门 | `share-import` 有候选账单与 `--go` | attach/merge/reject 三态落盘，reject 留理由 | 借，先补实现 |
| 发布裁决 | `share` 直接出明文 prompt | 加 `share-review` 隐私/质量闸门；团队走 space 密钥 | 借 |
| 成本账 | 无分享成本统计 | 记录包大小、导入重复率、召回 token、撤销率 | 借 |
| 独立验证 | 外部条目不区分来源与信任 | 外部内容默认 `external_untrusted`，不因作者升级 | 必须借 |

不借：

- 不把 respire 改成 Git 仓库：单写入者、密文存储，diff/merge 看不到明文；只借边语义。
- 不引入多 Agent 调度、并发工作副本、提名投票：个人记忆没有 swarm。
- 不因“观测到使用”重开 `read_heat`：观测≠因果，且当前已定“读取不加热”。
- 不把祖先覆盖率当能力提升，不做“分享让 Agent 变强”的宣传。
- 不把 `--go` 当安全边界：导入内容会被召回并注入上下文，必须隔离。

## 3. 分享单位：记忆 vs 经验

- **记忆（事实）**：用户/项目/环境的事实，适合小范围、按需分享；隐私风险高，默认不共享。
- **经验（可复用做法）**：条件命题，默认 `unverified`，不能当事实；更适合跨项目复用。

经验卡片 MVP 字段（只留 4 个必填）：

- `trigger`：什么条件下适用
- `action`：做了什么
- `boundary`：不适用边界/版本/环境
- `outcome`：`ok | fail | confounded | no_effect`（沿用 run-log 四值）

后续再加：`evidence`（run-log / source_refs）、`counter`（已知失败路径）、`task_signature`、`valid_until`、`authored_by/at`。
卡片应挂成因果树上的独立节点，不塞进 tags。

## 4. 分享包 manifest v2

```json
{
  "v": 2,
  "bundle_id": "uuidv7",
  "created_at": "ISO-8601",
  "author": {"user": "...", "device": "..."},
  "scope": "personal|project|team|dept",
  "root": "source-root-id",
  "items_digest": "sha256",
  "enc": "none|space-v1",
  "items": [
    {
      "origin_entry_id": "...",
      "parent_id": "...",
      "title": "...",
      "content": "...",
      "kind": "...",
      "tags": ["..."],
      "importance": 60,
      "status": "valid|unverified",
      "edges": [
        {"kind": "informed_by", "to": "origin-entry-id", "confidence": "declared"}
      ]
    }
  ],
  "evidence": {"run_logs": [], "source_refs": []},
  "cost": {"items": 0, "tokens_est": 0, "read_before_write": 0}
}
```

原则：

- 包只带决策所需：标题、边界、状态、证据摘要、成本；正文可按需展开，不默认全量塞 prompt。
- `origin_entry_id` 必须保留；导入端不得用新 uuid 覆盖来源。
- `enc=none` 只用于个人手动复制；团队/空间分享必须走 space 密钥，不把密文塞 prompt。
- 包内 `edges` 只表达“声明”，不自动升级为因果或已验证。

## 5. 导入决策与信任状态机

三态：

1. **attach**：挂到已有同主题节点，作为证据/子经验；保留 origin。
2. **merge**：仅同 kind 且 trigger 高重合；合并后保留两条 origin，可回滚。
3. **reject**：不写正文，只写导入回执与理由（重复/不适用/隐私/低质）。

状态机：

```
导入 → external_untrusted + unverified + active
  ├─ attach → attached（仍 external_untrusted）
  ├─ 本地复现（同 task/env ≥2 次 ok、0 次 fail）→ corroborated
  ├─ 用户明确确认 → trusted
  ├─ 过期 → needs_review
  └─ 来源撤销 → source_revoked（停止召回，不删行）
```

约束：

- 信任不因作者是同事、引用多、被召回多而升级。
- “corroborated”必须有本地 run-log 证据；没有 task_id/env_hash 时只显示次数与最近状态，不打 trusted 标签。
- 外部内容默认不参与排序加权；只做标注与过滤。

## 6. 负知识

- `fail/no_effect/confounded` 不是“这条经验是假的”，而是“在此边界内不可用/未定”。
- 存 `outcome + scope + evidence`，默认不进正向召回；查询命中相同 task/env 时作为警告返回。
- 不与 `kind` 混用；不建议用 `status=refuted` 表达，避免破坏现有 kind/status 过滤。
- 撤销证据要能沿来源边找到下游结论：`share-revoke` 标记 origin，`informed_by` 下游标 `source_revoked` 并降权/排除。

## 7. 三层证据与度量

只做审计，不做排序：

1. **声明层**：导入条目有多少带 origin / informed_by。
2. **观测层**：被 `recall`、`show`、`run-log` 实际使用的次数与最近时间。
3. **结果层**：最终 `run-log ok` 或后续记忆的 `informed_by` 是否能回溯到该 bundle。

配套指标：包大小、导入重复率、误挂率、撤销率、召回 token、过期率。
禁止把“被召回”写成“有用”，把“祖先关联”写成“导致成功”。

## 8. 现有实现缺口（代码事实）

- `share`：`SharePayload{v,kind,root_title,created_at,device,source_user,items}`（`respire-cli/crates/app/src/share.rs:64-77`）；仅子树父链 + 子树内 `see_also`（`share.rs:296-307`、`service.rs:2063-2085`）；`1MEMSHARE1:` + base64(gzip(JSON)) 明文（`share.rs:87-94`，注释 `share.rs:9`）；无脱敏闸门。
- `share-import`：候选阈值 merge≥0.50、parent 0.35–0.50（`respire-core/crates/core-ffi/src/analysis.rs:303-304`）；attach/new root 可落库（`service.rs:2210-2215`、`2279-2285`）；**merge 未实现**，只提示手跑 `remember --merge-ids`（`service.rs:2267-2273`）；`duplicates: 0` 硬编码（`service.rs:2341`）；`--parent` 跳过冲突检查（`service.rs:2217-2221`）；`--title` 只改 `payload.root_title` 与账单 JSON，不改进库条目标题（`main.rs:5931-5936`，已在 upstream `main` 复核）；`source_user` 导入端不读取；不可撤销。
- 存储：有 `parent_id/importance/valid_until/review_at/source_refs`（`respire-cli/crates/storage/src/transport/local.rs:968-990,1077-1090`）；无 `origin_bundle_id/origin_entry_id/informed_by/trust_level/status/revoked_at/outcome_refs`。迁移用 `PRAGMA table_info` + `ALTER TABLE ADD COLUMN` 幂等（`local.rs:915-1091`）。
- 同步：明文列 `#[serde(skip)]`，只传 ciphertext（`respire-cli/crates/protocol/src/lib.rs:266-321`），pull 由 `hydrate_local` 重建（`respire-cli/crates/crypto/src/engine.rs:416-434`）。**只加本地列会被 LWW 覆盖或他机丢失**。
- space：独立账号 + 独立 super key/data dir（`respire-cli/cli/src/space.rs:1-19`）；server 无 share 端点；kick 不回收已解密历史；跨空间导入/撤销不支持。
- 召回：`recall/search` 同一候选集，无 trust/status 过滤（`service.rs:113-127,196-200`）；导入条目会 bump 使用计数（`service.rs:202`）。
- `read_heat`：Core 默认 `true`（`respire-core/crates/memory-core/src/query.rs:301`），CLI 未配置时沿用；与“默认关”的已定决策不一致，需先确认是否未落地。

## 9. MVP 实施

### P0：正确性与安全（先做，约 2–4 人日，估算）

1. 修 `share-import`：真 merge 或删掉 merge 提示；按 `items_digest`/origin 幂等去重；`--parent` 必过冲突检查；`--title` 真正生效；reject 写导入回执；支持回滚。
2. 外部内容隔离：导入条目 `external_untrusted + unverified`；默认不自动 sync；注入时加分隔与警告；不自动执行导入的过程性内容。
3. `share-list` + `share-revoke`：列 bundle/entry/状态；撤销标记 origin，`informed_by` 下游 `source_revoked`；不物理删行，保留审计。
4. origin 进密文 payload，不只放本地明文列；同步后仍可追溯。

### P1：经验分享（约 3–5 人日，估算）

5. manifest v2：bundle_id、author、scope、items_digest、edges、evidence、cost；保留 v1 解码兼容。
6. 经验卡片四字段进 share-import 候选账单；attach/merge/reject 三态。
7. `share-review`：密钥/凭据/个人信息/客户数据扫描 + 体积/冲突检查；默认只报告不写。

### P2：度量（约 2–3 人日，估算）

8. `share-report`：声明/观测/结果三层计数 + 成本；只读。
9. E1/E2/E3 门禁（见下）。

### MVP 最小改动面（工部建议）

- 改 3 个文件：`respire-cli/crates/app/src/share.rs`、`respire-cli/crates/app/src/service.rs`、`respire-cli/cli/src/main.rs`
- 加 1 张表：`share_origins`（origin 同步进密文）
- 加 2 个命令：`share-list`、`share-revoke`

## 10. 验收实验（均未执行，属门禁条件）

- **E1 幂等与迁移**：同一包导入两次 + 换机同步。合格：重复行 0、origin 保留 100%、可回滚、`--parent` 绕过 0。
- **E2 导入判定**：20 包盲测（10 真重复/10 新主题）。合格：merge 精确≥0.9、误挂≤5%；样本来源与标注口径需先定义。merge 未实现前，该指标不可验证。
- **E3 分享效果**：30 题（15 可答/15 过期），对照“无分享 / 有标记 / 无标记”并交叉 `read_heat`。指标：准确率、有害采纳、注入成功、token。未做因果消融前，只能报告差异，不能宣称提升。

## 11. 风险清单

1. **LWW 洗掉来源**：origin 只放本地列会在同步/换机丢失；必须进密文 payload。
2. **自动同步外泄**：导入后若 `auto_sync` 开启，他源明文会作为本机记忆上传；MVP 默认禁止导入即同步。
3. **导入即改写元数据**：当前导入覆盖 `user/computer/project/emotion`，origin 在写入瞬间丢失；必须先写 origin 再规范化。
4. **prompt injection / 记忆投毒**：外部正文会被召回注入；`--go` 不是安全边界；需 `external_untrusted` 隔离、分隔标记、人工确认。
5. **`--parent` 绕过冲突检查**：可把新包硬挂到错误父节点；必须统一冲突检查。
6. **重复导入**：`duplicates: 0` 硬编码，重复包必产重复行。
7. **信任标签幻觉**：run-log 缺 task_id/基线时，`trusted` 不可信；MVP 只显示次数/最近状态。
8. **read_heat 口径不一致**：代码默认 true，与已定“默认关”冲突；不修则效果实验会被热度污染。
9. **经验卡片过度设计**：MVP 只留 trigger/action/boundary/outcome，其余后置。

## 12. 如果只做三件事

1. **先修 share-import 的正确性与安全**：幂等、真 merge 或删提示、`--parent` 检查、外部隔离、撤销。
2. **origin 进密文 + `share-list/share-revoke`**：让“分享了什么、从哪来、能否撤回”可查。
3. **经验卡片四字段 + 三态导入**：把“分享文本”变成“分享可复用做法”，但默认 `unverified`。

## 13. 未决项

- `read_heat` 默认 false 是否有明确决策记录？当前代码是 true；需 owner 确认后修默认值 + 回归，再做效果实验。
- `informed_by` 是否值得作为第四套边？若不能回答一个独立问题，先复用 `see_also` 加方向/置信，避免四套边漂移。
- `share_origins` 是否必须跨设备同步？建议必须；否则换机后来源与撤销链断裂。
- 信任等级是否需要 `trusted`？在无独立复现前，建议只保留 `unverified/corroborated/source_revoked`。
- MVP 人日为粗估，未做排期；以实际代码改动与测试为准。
