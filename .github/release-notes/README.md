# 历史发布说明（已冻结）

这里存放 0.2.0 – 0.3.3 发布时写下的原始公告，仅作历史归档，**不再更新**。

- 变更日志的唯一来源是仓库根目录的 [CHANGELOG.md](../../CHANGELOG.md)：Keep a Changelog 格式，覆盖 0.1.0 至今的全部版本。
- 发布 workflow（[`../workflows/release.yml`](../workflows/release.yml)）只用 `scripts/changelog.sh` 从 `CHANGELOG.md` 提取发布说明，不读取本目录。
- 新版本的说明请写进 `CHANGELOG.md` 的版本小节，不要在这里新建文件（`tests/docs.rs` 会校验 workflow 不再引用本目录）。
