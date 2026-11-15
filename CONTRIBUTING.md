# 贡献指南

欢迎参与 typewords-tui！这是从 [TypeWords](https://github.com/zyronon/TypeWords) 拆分出的独立 Rust 终端客户端，仓库以「单人维护 + 社区 PR」的方式迭代。

## 开始之前

1. 阅读 [文档索引](docs/README.md)，按你的目标选择文档。
2. 开发环境、构建 / 测试命令、代码风格与发布流程见 [开发指南](docs/development.md)。
3. 想改动某个模块，先看 [总体架构](docs/architecture.md) 的分层、生命周期与子系统契约；数据格式见 [存档与数据格式](docs/persistence.md)。

## 提交前检查

    cargo fmt --all --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --locked

全部通过后再提交。修 bug 请附带能复现的测试；新增交互 / 设置项 / 流程时，同步更新对应文档（见 [开发指南 · 文档维护](docs/development.md#10-文档维护)）。

## 提交信息

沿用 conventional 前缀（如 `feat(tui):`、`fix(paths):`、`docs:`），一个提交聚焦一件事，说明动机与影响面。

## 变更日志

用户可见的改动（新功能、行为变化、修复、破坏性变更）请在 [CHANGELOG.md](CHANGELOG.md) 的 `## [Unreleased]` 下按 Keep a Changelog 的分类（新增 / 变更 / 弃用 / 移除 / 修复 / 安全）补一条，写清「改了什么、对使用者有什么影响」；纯内部重构与测试不单列。发版时由维护者把整段移到新的版本标题下，发布 workflow 用该小节生成 GitHub Release 正文——没有条目的版本发不出去。

## 文档贡献

- 文档以源码为准；发现文档与代码不一致，欢迎直接提 PR 修正（先改文档，若确认是代码缺陷再改代码）。新增文档后在 [docs/README.md](docs/README.md) 登记——`cargo test` 会校验相对链接、索引完整性、文档版本、源码路径与 [CHANGELOG.md](CHANGELOG.md) 的格式（见 `tests/docs.rs`）。
- 修改界面后按 [开发指南 · 截图与视觉检查](docs/development.md#6-截图与视觉检查) 重新生成截图。

## 许可证

本项目遵循 [GPL-3.0](LICENSE)。提交贡献即表示同意以同一许可证发布；词库数据来自上游 TypeWords 仓库，遵循同一许可证。
