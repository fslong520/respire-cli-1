# Memory sharing and experience exchange — design proposal (2026-11-02)

English summary: This proposal maps GitSwarm (arXiv:2610.04862) onto Respire's existing
`share` / `share-import` surface. The transferable idea is not "share text and agents get
stronger"; it is a provenance-carrying, status-aware import: keep the origin entry, separate
the structural parent from declared semantic dependencies, keep declared / observed / outcome
evidence apart, retain failed paths as scoped negative knowledge, and gate publication without
treating publication as correctness. Current v1 exports plaintext content plus metadata,
unknown fields and internal supersession/cross-reference links. Import validates metadata,
strips sender verification and applies existing write/source/sync gates, but does not establish
an authenticated cross-library origin or share-specific trust/revocation protocol. Successful
imports mint fresh local IDs; semantic conflict refusal is not origin-based idempotence.
Status: design proposal, not implemented. Facts below are pinned to `respire-cli` main
`0f6bf9ac30f8e93a12f669c7495f0ada6689ca25` (published CLI DEV6, Core SDK dev.16).
This PR changes documentation only: no v2 importer, commands, quarantine, trust upgrade,
revocation or read-heat behavior is implemented, and no section-10 experiment has been run.

---

# GitSwarm 借鉴与 respire 分享记忆/经验设计（2026-11-02）

> 结论：值得借，但只借“来源、边、证据分层、负知识、发布闸门”，不借 Git 仓库、多 Agent 调度、投票与“祖先覆盖率=提升”。
> 当前 `share/share-import` 更像“导出文本包”，还不是“分享经验”。MVP 应先修正确性与安全，再谈经验卡片和效果度量。

## 0. 一句话结论

GitSwarm 真正有价值的不是“共享记忆让 Agent 变强”，而是它把推理过程变成**可持久、可分支、可归因的贡献图**，并把“声明用了什么 / 实际用了什么 / 最终结果继承了什么”分成三层证据。respire 应把“分享”从**复制文本**升级为**携带来源与状态的受控导入**，但不要把自己改成 Git 或多 Agent 运行时。

## 1. 文章机制与证据边界

文章对象：[GitSwarm: Decentralized Compounding Inference，v1](https://arxiv.org/html/2610.04862v1)（arXiv:2610.04862v1）。论文结果不是本产品的能力、排期或策略授权。

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
| 贡献账本 | 因果树、内部 supersession/`see_also`，share 还带完整 metadata 与未知扩展；缺已认证跨库 origin | 分享包带来源、边、状态、证据摘要 | 借，缩水 |
| parent vs informed_by | `parent` 是因果；`see_also` 对称；`source_refs` 是验证引用 | 保留现有对称 `see_also` 与来源引用语义；若确需 `informed_by`，另定版本化有向关系，不能静默重解释旧边 | 修订 |
| 三层证据 | `query-log`、`run-log`、`history` 各自存在 | 只做审计报表，不合成热度、不用于排序 | 借原则 |
| 失败留痕 | `run-log` 有 fail/no_effect/confounded | 负知识带 scope，默认不进正向召回，匹配场景作警告 | 借，改语义 |
| 三态闸门 | `share-import` 有候选账单与 `--go` | attach/merge/reject 三态落盘，reject 留理由 | 借，先补实现 |
| 发布裁决 | `share` 出含正文/元数据的明文 prompt；导入已有 metadata gates | 提议额外审查，不能把 space 加密等同身份认证/撤回明文 | 借 |
| 成本账 | 无分享成本统计 | 记录包大小、导入重复率、召回 token、撤销率 | 借 |
| 独立验证 | 导入剥离 verification；尚无分享专用来源/信任隔离协议 | 拟加本地来源标注；不得导入即升级信任 | 必须借 |

不借：

- 不把 respire 改成 Git 仓库：单写入者、密文存储，diff/merge 看不到明文；只借边语义。
- 不引入多 Agent 调度、并发工作副本、提名投票：个人记忆没有 swarm。
- 不因“观测到使用”调整 `read_heat`：观测≠因果；当前代码默认 true。本提案没有可核验的“默认关”产品决定，不据此修改行为。
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

## 4. 分享包 manifest v2（拟议格式，当前 CLI 不支持）

```json
{
  "v": 2,
  "bundle_id": "uuidv7",
  "created_at": "ISO-8601",
  "author": {"user": "...", "device": "..."},
  "scope": "project",
  "root": "source-root-id",
  "origin_namespace": {"issuer": "claimed-issuer", "library": "source-library"},
  "digest_scheme": "rfc8785-jcs-sha256-v2",
  "items_digest": "<canonical-manifest-sha256>",
  "enc": "none",
  "items": [
    {
      "origin_entry_id": "...",
      "origin_revision": "...",
      "parent_id": "...",
      "title": "...",
      "content": "...",
      "kind": "...",
      "tags": ["..."],
      "importance": "important",
      "declared_status": "unverified",
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
- 新本地 UUID 可以独立生成；origin 的键应是 `(issuer, library, entry_id, revision)`，不能只用跨库可碰撞的 entry UUID，也不能以本地新 ID 覆盖原始声明。
- 拟议 digest 使用 [RFC 8785 JCS](https://www.rfc-editor.org/rfc/rfc8785) 的 UTF-8 规范序列化。覆盖解密后的逻辑 manifest（包括版本、namespace、root、items/修订、edges），仅移除 `items_digest` 本身；传输 envelope 的 nonce/ciphertext 不参与该逻辑 digest。对象重复键、非法 Unicode/数字拒绝；数组顺序保留并参与 hash，不额外做 Unicode 归一化。版本化 scheme 必须在实现前用跨语言向量冻结。
- 相同 origin 修订出现不同 digest 时是显式冲突，不能覆盖或静默去重；不同 issuer/library 的同名 ID 不合并。digest 提供内容完整性/相等性，不证明来源真实；`author`、`device`、issuer 字符串都只是声明。要认证发行者，需另定密钥绑定、签名/验证、授权及失效协议，且未认证来源保持未验证。
- `enc=none` 仍暴露正文和 metadata；space 共享密钥仅是保密/访问控制的一部分，不证明作者身份，不撤回已取得的明文。`space-v1` envelope 只是待设计能力，不能当作已有 share 加密格式。
- 包内 `declared_status`、`evidence`、`verification` 和过程性 instructions 都是外部声明，不能变成本地验证、执行授权或更高指令。保留当前导入剥离 sender verification 的原则；本地复核必须产生独立、绑定记录修订/范围的证据。
- 当前前缀 `1MEMSHARE1:` 接受 payload `v=1`，不支持的版本会被拒绝。未来 v2 需另定版本协商与 v1 兼容/降级规则；legacy v1 缺认证 origin 时不能补造来源或偷偷赋予 v2 信任。
- 未来 importer 必须在分配/解压和写入前限制编码、压缩/解压字节、条目数、正文/字段大小及图复杂度；验证重复 ID、根/父链、环、悬空边、跨 namespace 引用及内容冲突后再 mutation。当前 decoder 的 `read_to_end` 不是这些边界的实现。

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
  ├─ 用户明确、可审计的本地复核 → local_reviewed（范围受限，不授予执行权限）
  ├─ 过期 → needs_review
  └─ 来源撤销 → source_revoked（停止召回，不删行）
```

约束：

- 以下状态机均未实现。信任不因作者是同事、引用多、被召回多、包内自填 status/evidence 或一次导入而升级。导入不得静默提升本地信任或自动执行条目中的命令。
- “corroborated”必须有本地 run-log 证据；没有 task_id/env_hash 时只显示次数与最近状态，不自动授予 trusted 标签，也不能把复现次数变成授权。
- 外部内容默认不参与排序加权；只做标注与过滤。

## 6. 负知识

- `fail/no_effect/confounded` 不是“这条经验是假的”，而是“在此边界内不可用/未定”。
- 存 `outcome + scope + evidence`，默认不进正向召回；查询命中相同 task/env 时作为警告返回。
- 不与 `kind` 混用；不建议用 `status=refuted` 表达，避免破坏现有 kind/status 过滤。
- 拟议 `share-revoke` 先定义接收方本地抑制：标记有命名空间的 origin，下游标记需复核，并按明确策略抑制召回。传播撤销需已认证 issuer/接收回执/离线规则；不能保证删除别人的离线副本、截图、备份或已解密历史，也不能据此声称撤回已取得的知识。当前尚无该命令。

## 7. 三层证据与度量

只做审计，不做排序：

1. **声明层**：导入条目有多少带 origin / informed_by。
2. **观测层**：被 `recall`、`show`、`run-log` 实际使用的次数与最近时间。
3. **结果层**：最终 `run-log ok` 或后续记忆的 `informed_by` 是否能回溯到该 bundle。

配套指标：包大小、导入重复率、误挂率、撤销率、召回 token、过期率。
禁止把“被召回”写成“有用”，把“祖先关联”写成“导致成功”。

## 8. 现有能力与确认缺口（固定源码事实）

以下 CLI 文件均指向 main `0f6bf9ac30f8e93a12f669c7495f0ada6689ca25`，以符号/字段为准；不能把提案字段当作当前协议。

- **v1 导出范围**：[`ShareItem` / `items_from`](https://github.com/risense-ai/respire-cli/blob/0f6bf9ac30f8e93a12f669c7495f0ada6689ca25/crates/app/src/share.rs) 含正文、标题、kind/tags、时间、字符串 `importance`，并 flatten 完整 `MemoryMeta` 与 `unknown_fields`。metadata 可含 `source_refs`、`artifact`、有效期/复核期和 `verification`；还导出子树内部 `supersedes`、`superseded_by`、`see_also`，外部结构父链/关系被截去。`source_user`/`device` 只是未认证声明，不是持久的跨库来源身份。`1MEMSHARE1:` + base64(gzip(JSON)) 是明文编码，不是加密；没有专用 share 脱敏审查。隐私检查须覆盖正文与全部 metadata/未知字段/证据引用/仓库路径，不只检查文本。
- **现有导入保护**：[`import_share_payload`](https://github.com/risense-ai/respire-cli/blob/0f6bf9ac30f8e93a12f669c7495f0ada6689ca25/crates/app/src/service.rs) 解析 kind/importance、校验 metadata，生成新本地 ID，重建父链并重映射 supersession/`see_also`，明确清除发送者 `verification`；提交前经过 `metadata_gate::ensure_write` / `ensure_sources`，封装后使用 batch import。不能写成“完全无校验/状态保护”。这些校验不等于认证分享者、幂等导入或分享专用 quarantine。
- **确认缺口**：未给 `--parent` 且未 `--force` 时会做语义冲突检查并可拒绝，显式 `--parent` 跳过这项冲突判断；不能笼统称每次重复导入必成功。成功重复导入仍会产生新本地 ID，`duplicates: 0` 不等于已完成 origin/digest 去重。share-import 的 merge 路径只给出后续手工 merge 提示，未实现自动 merge；`--title` 改 `payload.root_title`/候选账单而非逐条入库 title。没有分享专用 origin 清单/撤销协议。
- **元数据与同步**：[`MemoryMeta`](https://github.com/risense-ai/respire-cli/blob/0f6bf9ac30f8e93a12f669c7495f0ada6689ca25/crates/protocol/src/metadata.rs) 已有有效期、来源引用、artifact 和 verification，不应与拟议的 share-specific trust/revocation 状态混称“完全无状态”。[`MemoryEngine::seal` / `hydrate_local`](https://github.com/risense-ai/respire-cli/blob/0f6bf9ac30f8e93a12f669c7495f0ada6689ca25/crates/crypto/src/engine.rs) 把记录/metadata/未知扩展保存在加密 payload，明文本地列不直接当作同步内容。成功导入且 autosync/remote 配置允许时可同步 **sealed ciphertext**；[`metadata_gate`](https://github.com/risense-ai/respire-cli/blob/0f6bf9ac30f8e93a12f669c7495f0ada6689ca25/crates/app/src/metadata_gate.rs) 的 isolated 部署可阻止含相应 metadata 的库同步。仍须审查解密接收方范围；仅加本地 origin 列会在换机/hydration/LWW 后丢失，不能替代版本化加密来源协议。
- **space 边界**：现有 space 账号/key/data-dir 隔离不构成分享者签名认证；移除成员不能收回已取得的密钥、离线副本或已解密历史。这里提出的跨库来源撤销/传播还未实现。
- **召回与 read heat**：当前已有 metadata 验证/有效性语义，但未实现本提案的分享来源/quarantine/trust 过滤协议。Core `QuerySettings::default()` 的 `read_heat` 为 true；CLI 查询设置装配未指定时沿用。没有可核验的“默认关闭”决定，本 PR 不调整读取计数、热度、排序或来源信任。效果实验必须记录实际设置，变更默认值须另行授权及回归。

## 9. 候选实施路线（均未实现，不是本次发布承诺）

### P0：正确性与安全（须另行评审、授权和排期）

1. 修 `share-import`：真 merge 或删掉 merge 提示；按 `items_digest`/origin 幂等去重；`--parent` 必过冲突检查；`--title` 真正生效；reject 写导入回执；定义可审计的本地补偿/回滚边界，不能许诺撤销对方已复制的数据。
2. 外部内容隔离：在保留现有 metadata gates/verification 清除的基础上，设计显式的本地 quarantine/复核及同步策略；拟议 `external_untrusted` 不是现有导入标签。默认同步策略变更须另行确认；不自动执行过程性内容。
3. `share-list` + `share-revoke`：列 bundle/entry/状态；撤销标记 origin，`informed_by` 下游 `source_revoked`；不物理删行，保留审计。
4. origin 进密文 payload，不只放本地明文列；同步后仍可追溯。

### P1：经验分享（候选设计，工期未估定）

5. manifest v2：bundle_id、author、scope、items_digest、edges、evidence、cost；保留 v1 解码兼容。
6. 经验卡片四字段进 share-import 候选账单；attach/merge/reject 三态。
7. 拟议 `share-review`：正文、metadata/unknown fields、source_refs、artifact 与 verification 的隐私/冲突审查及体积检查；先定义误报/漏报和可用扫描能力，不声称当前已有自动凭据扫描。默认只报告不写。

### P2：度量（候选设计，工期未估定）

8. `share-report`：声明/观测/结果三层计数 + 成本；只读。
9. E1/E2/E3 门禁（见下）。

### 改动面与依赖（不能按“三文件、单表、两命令”交付整个协议）

- share.rs/service.rs/main.rs 只是入口，不是完成加密 provenance、认证、撤销、quarantine 和召回隔离的充分改动面。
- 实现前需盘点 protocol/schema、加密 seal/hydrate 与旧 writer 保留、storage 迁移与原子提交/回执、同步兼容、CLI/MCP/HTTP 暴露及权限、来源与关系校验、召回过滤和验收。Core/SDK 仅在相应契约确有需要时配套修改；文档事实修订本身不需要新 Core。
- `share_origins` 之类本地索引若采用，权威 origin 仍须在受版本/兼容约束的加密 payload；不能仅增加一张明文表。
- 所有 `share-list`、`share-revoke`、`share-review`、`share-report`、manifest v2 和新信任状态都尚未实现。本 PR 不是实现整个平台的产品授权，也不改变现有 API、存储、默认同步或 read_heat。

## 10. 验收实验（均未执行，属门禁条件）

- **E1 幂等与迁移**：同一包导入两次 + 换机同步。合格：重复行 0、origin 保留 100%、可回滚、`--parent` 绕过 0。
- **E2 导入判定**：20 包盲测（10 真重复/10 新主题）。合格：merge 精确≥0.9、误挂≤5%；样本来源与标注口径需先定义。merge 未实现前，该指标不可验证。
- **E3 分享效果**：30 题（15 可答/15 过期），对照“无分享 / 有标记 / 无标记”并交叉 `read_heat`。指标：准确率、有害采纳、注入成功、token。未做因果消融前，只能报告差异，不能宣称提升。

## 11. 风险清单

1. **LWW 洗掉来源**：origin 只放本地列会在同步/换机丢失；必须进密文 payload。
2. **同步接收范围**：现有导入先 seal 为密文，按 autosync/remote 设置及 metadata isolated gate 决定能否同步；不是把 share 的明文包直接上传。来源/敏感元数据仍可能被有解密能力的接收端看到，后续 quarantine/同步策略须明确授权。
3. **来源声明未持久命名空间化**：导入规范化 `user/computer/project/emotion`，没有持久的 issuer/library/entry/revision 归因协议；但现有 metadata、unknown fields 及内部关系并非全部丢失。来源应与本地身份分开，且不能自行声称经过认证。
4. **prompt injection / 记忆投毒**：外部正文会被召回注入；`--go` 不是安全边界；需 `external_untrusted` 隔离、分隔标记、人工确认。
5. **`--parent` 绕过冲突检查**：可把新包硬挂到错误父节点；必须统一冲突检查。
6. **重复导入**：成功导入生成新 ID，尚无 origin/digest 幂等回执；未指定 parent/force 的语义冲突可阻止重复成功，不能用这层拒绝替代幂等协议。
7. **信任标签幻觉**：run-log 缺 task_id/基线时，`trusted` 不可信；MVP 只显示次数/最近状态。
8. **read_heat 实验控制**：代码默认 true，本提案没有改变默认值的决定。实验应显式记录/控制该设置，不能虚构“默认关”承诺或借本提案修改排序行为。
9. **经验卡片过度设计**：MVP 只留 trigger/action/boundary/outcome，其余后置。

## 12. 如果只做三件事

1. **先修 share-import 的正确性与安全**：幂等、真 merge 或删提示、`--parent` 检查、外部隔离、撤销。
2. **命名空间 origin 进密文 + 拟议清单/本地撤销**：让分享与接收方抑制可查，不承诺删除离线副本或作者身份已被认证。
3. **经验卡片四字段 + 三态导入**：把“分享文本”变成“分享可复用做法”，但默认 `unverified`。

## 13. 未决项

- 是否改变 `read_heat` 默认值是独立产品决定；目前默认 true，本提案保持现状，先定义实验控制而非预设要改。
- `informed_by` 是否确有独立需求？现有 `see_also` 的对称语义保持不变；新增有向边须明确契约与 writer 兼容，不静默重用旧字段。
- `share_origins` 是否必须跨设备同步？建议必须；否则换机后来源与撤销链断裂。
- 信任等级是否需要 `trusted`？在无独立复现前，建议只保留 `unverified/corroborated/source_revoked`。
- 身份/issuer 认证、命名空间与 revision 生命周期、JCS 跨语言向量/碰撞冲突、v1 兼容及撤销传播均待详细评审。未做排期；不以原先人日粗估承诺交付。
