# 不可解析当前修订的受控恢复（待验收）

## 问题与范围

当前远端修订已 applied、但 payload 无法解析时，pull 记录远端 head 而不推进本地 sync_base；原 take-incoming 又要求二者相等，导致“先完成同步”无法解除阻塞。

本分支只修复客户端恢复出口。以官方 main 的 d5001f8 为基线，分支为 `fix/undecodable-head-recovery`。本次作为待验收草案提交评审；尚未部署，未对真实记忆执行 sync-resolve / sync-restore。Rust 与真实服务验收缺口见下文，不能将源码提交视为恢复成功。

## 行为

- epoch、head 检查和操作创建位于同一个 SQLite Immediate 事务。
- 只有持久 head 带解码错误、当前仍无法读取，且用户明确选择 take-incoming 时，允许替换不可解析 head；merge 继续拒绝。
- 存在本地 dirty、pending 操作或可继续发送的恢复操作时，拒绝覆盖。
- 候选仍须经现有解密与字段解析；有效删除候选保留删除意图。候选无效则整体回滚。
- 仅在操作首次提交事务前，把新操作的 base_rev 设置为用户审阅的 head；sync_base 保留最后真正应用的版本。随后使用既有服务端版本比较与 restore_op_id 协议，不新增服务端接口。
- 保留原记忆 ID、坏修订和候选历史。新表 sync_recovery_intents 只记录 epoch、候选版本、审阅 head 与 op_id，作为跨重试的幂等键，不含正文或密钥。
- 同一恢复请求重复调用复用原操作；服务端拒绝后不自动生成新的 op_id。已处理的 take-incoming 重试不再创建版本。
- sync-reset 会使旧操作失效：旧恢复请求明确报错；重新审阅新 head 后，已失效旧关联不会阻塞新的恢复。不要把 sync-reset 当成此问题的修复步骤。
- 本地排队、上传 applied 均不等于冲突已解决；processed 仍等待既有服务端 resolution 回执。

## 验证状态

已完成：

- 修改文件的 Rust 语法/格式检查、Git 补丁空白检查。
- 使用源码中的 SQL 和内存 SQLite 验证：初始化可重复执行；dirty 保护；新操作改基线但保留 sync_base 与历史；事务回滚；失败幂等键保留；重复键拒绝；reset 旧操作不复用。
- 独立代码复核；修正 reset 后幂等早退误报成功的问题。

原提交添加的 Rust 回归用例（维护验收已实际执行）：

1. 坏 head + 好候选恢复及同请求重复排队。
2. 内容 applied 后仍待 resolution 回执，完整回执后幂等重试。
3. head、epoch 改变及 merge 拒绝。
4. pending 与仅 dirty 的本地修改保护。
5. 可读但尚未应用的 head 仍要求先同步。
6. 坏候选导致事务回滚。
7. 模拟服务端并发冲突，保留未解决状态且重复请求不增加冲突。
8. 同 epoch reset 后旧恢复重试显式失效，不创建额外操作；覆盖 pending、已 applied 和已 conflict_saved 的内容回执。旧的未完成 resolution 关联失效，操作历史保留，新 head 可发起新的显式恢复。
9. 新客户端没有已应用基线时仍可显式恢复。

执行入口（需先按仓库说明配置匹配的 Core SDK 和 Windows 构建工具）：

```text
cargo test --locked -p respire_storage transport::local::conflicts::tests -- --test-threads=1
cargo test --locked -p respire_storage transport::local::sync::tests -- --test-threads=1
```

原提交阶段没有匹配 SDK 与完整依赖缓存，因此当时未能运行 Rust 测试。维护验收使用已验证的官方 SDK `1.0.2-dev.12`（源 `77fc566e47c6c283c0bde9192528debad23d6248`）和离线依赖，在隔离 HOME/库目录、无凭据环境运行全部 storage 测试：53 项通过，包括上述 9 项恢复回归。扩展的 reset 负向用例在修复前实际失败，修复后通过。

reset 的修复只取消与恢复意图绑定、尚未完成的 resolution outbox 关联；不删除操作、候选、坏版本或已处理的 resolution 历史。不能把这里的回执模拟或本地 Rust 通过当成真实服务端并发验收。

## 后续验收与未解决项

- 在具有官方匹配 SDK 的环境完成上述 Rust 测试，再在隔离账号验证真实服务端 CAS、回执丢失重试及并发更新。
- 通过前不替换当前 Runtime；实际恢复需独立备份与用户确认。当前旧 Runtime 不会因为源码分支存在而自动获得修复。
- 初始 head 的解析失败根因仍未知。本次未读取原始密文、未更改 payload 兼容规则。
- 安全解析诊断的阶段、类型、字段定位属于后续 crypto 层改进；本分支仅区分恢复前置条件错误，不新增可能泄露正文/密钥的底层错误输出。
