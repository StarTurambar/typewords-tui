# 快捷键

本文是 typewords-tui 快捷键的权威参考：键位表的数据模型、全部动作与默认绑定、改绑方式，以及与上游 TypeWords 的差异。

按键速查（页面级导航键）见 [使用手册 · 按键参考](usage.md)；练习页的键位含义见 [使用手册 · 练习流程](usage.md)。

## 1. 键位表模型

快捷键是**数据**而不是代码里的 `match` 分支：每个可配置动作（action）对应一个键位（key），整体存在设置存档的 `shortcutKeyMap` 字段里。

- 动作枚举：`src/domain/shortcut.rs` 的 `ShortcutAction`，与上游 `app/core/types/enum.ts` 的 `ShortcutKey` **一一对应**（42 个）。
- 键位表示：`ShortcutKey`（修饰键 `ctrl`/`alt`/`shift` + 基础键 `BaseKey`）。基础键不依赖终端 crate，领域层因此可以在不知道终端事件类型的情况下做匹配。
- 键位字符串格式与上游 `getShortcutKey` 一致：`Ctrl+` / `Alt+` / `Shift+` 前缀按此顺序拼接；方向键写作 `➡` `⬅` `⬆` `⬇`；空格写作 `Space`；ASCII 字母统一大写。
- 查表：`ShortcutMap::actions_for` 把按键反查成动作列表。**同一个键位可以绑定多个动作**（例如 `1` 既是自测的「认识」也是选择题的 A），具体取哪一个由当前场景决定。

## 2. 改绑

1. 设置页 → 「通用」分区 → 「快捷键」→ Enter。
2. ↑↓ 选择动作，Enter 开始捕获，敲下新键位即完成改绑；Esc 取消捕获。
3. Delete 清除该动作的绑定，`r` 恢复该动作的默认键位，`R` 恢复全部默认键位，Esc 返回设置页。

改绑会立即生效并写入 `setting.json`。若新键位已被别的动作占用，会提示「该键位已被「X」占用」并保持原绑定。

## 3. 默认键位

下表与上游 `app/core/config/env.ts` 的 `DefaultShortcutKeyMap` 逐条一致，除「差异」一节列出的两处。

| 动作 | 说明 | 默认键位 | 状态 |
| --- | --- | --- | --- |
| EditArticle | 编辑文章 | `Ctrl+E` | 暂不可用 |
| PlayWordPronunciation | 播放单词发音 | `Ctrl+P` | 暂不可用 |
| ToggleShowTranslate | 切换翻译 | `Ctrl+Z` | 可用 |
| ToggleDictation | 切换默写遮罩 | `Ctrl+I` | 可用 |
| ToggleTheme | 切换主题 | `Ctrl+Q` | 暂不可用 |
| ToggleConciseMode | 切换简洁模式 | `Ctrl+G` | 暂不可用 |
| ToggleToolbar | 切换工具栏 | `Ctrl+B` | 可用 |
| TogglePanel | 切换词表面板 | `Ctrl+L` | 可用 |
| RandomWrite | 随机默写 | `Ctrl+R` | 可用 |
| Previous | 上一个词 | `Ctrl+⬅` | 可用 |
| Next | 下一个词 | `Ctrl+➡` | 可用 |
| PreviousChapter | 上一章 | `Alt+⬅` | 暂不可用 |
| NextChapter | 下一章 | `Alt+➡` | 暂不可用 |
| ShowWord | 显示单词 | `Escape` | 可用 |
| Ignore | 跳过单词 | `Tab` | 可用 |
| ToggleSimple | 标记已掌握 | `` ` `` | 可用 |
| ToggleCollect | 收藏 | `Enter` | 可用 |
| CollectToDict | 收藏到指定词典 | `Shift+Enter` | 暂不可用 |
| NextStep | 跳到下一阶段 | `Shift+➡` | 可用 |
| RepeatChapter | 重复本章 | `Ctrl+Enter` | 可用 |
| DictationChapter | 默写本章 | `Alt+Enter` | 暂不可用 |
| KnowWord | 自测：认识 | `1` | 可用 |
| UnknownWord | 自测：不认识 | `2` | 可用 |
| MasteredWord | 自测：已掌握 | `3` | 可用 |
| ChooseA | 选择 A | `1` | 暂不可用 |
| ChooseB | 选择 B | `2` | 暂不可用 |
| ChooseC | 选择 C | `3` | 暂不可用 |
| ChooseD | 选择 D | `4` | 暂不可用 |
| WordTestingNext | 单词测试：下一题 | `Space` | 暂不可用 |
| SelfTestingChooseA | 自测选择 A | `Alt+1` | 可用 |
| SelfTestingChooseB | 自测选择 B | `Alt+2` | 可用 |
| SelfTestingChooseC | 自测选择 C | `Alt+3` | 可用 |
| SelfTestingChooseD | 自测选择 D | `Alt+4` | 可用 |
| PlaySentence1..9 | 播放例句 1..9 | `Ctrl+1` … `Ctrl+9` | 暂不可用 |

「暂不可用」表示该动作的绑定仍然保留（以便与上游存档互通），但当前版本还没有对应功能，实现后会去掉标记。

### 终端兼容别名

部分终端会吞掉 `Ctrl+方向键`，另一些把 `Ctrl+J` 报成换行。因此额外提供两个**不落盘**的别名：

| 动作 | 别名 |
| --- | --- |
| Next（下一个词） | `Ctrl+N` |
| NextStep（跳到下一阶段） | `Ctrl+J` |

别名与主键位同时生效；用户改绑主键位不会影响别名。

## 4. TUI 专有键位

下列键位在上游没有对应动作，或功能尚未实现，因此不走键位表：

| 键位 | 作用 | 说明 |
| --- | --- | --- |
| `Ctrl+C` | 保存并退出练习 | 终端保留键，不可改绑 |
| `Ctrl+P` | 暂停 / 恢复计时 | 上游该键位是「播放单词发音」；发音功能落地后移交 |
| `Ctrl+E` | 显示 / 隐藏例句与词源 | 上游该键位是「编辑文章」 |
| `?` | 打开帮助 | 练习页内保留当前会话 |
| `1`~`5`、`d`/`s`/`t`、`m`、`f`、`+`/`-`、`q` | 页面级导航与练习入口 | 见 [使用手册 · 按键参考](usage.md) |

## 5. 与上游的差异

| 差异 | 上游 | TUI | 原因 |
| --- | --- | --- | --- |
| 简洁模式 | `Ctrl+M` | `Ctrl+G` | `Ctrl+M` 在多数终端等同回车，无法与 Enter 区分 |
| 练习内暂停计时 | 无（上游有「播放发音」） | `Ctrl+P` | 发音功能落地前借用该键位 |
| 例句 / 词源开关 | 无（上游有「编辑文章」） | `Ctrl+E` | 文章编辑功能落地前借用该键位 |
| 下一个词 / 下一阶段 | `Ctrl+➡` / `Shift+➡` | 同左，另加 `Ctrl+N` / `Ctrl+J` 别名 | 部分终端不支持 `Ctrl+方向键` |

其余动作的默认键位、字符串格式与存档字段名都与上游一致，Web 端导出的 `shortcutKeyMap` 可以直接使用。

## 6. 存档格式

`setting.json` 的 `shortcutKeyMap` 字段，类型为 `Record<string, string>`（动作名 → 键位字符串），与上游 `SettingState.shortcutKeyMap` 同构：

```json
{
  "val": {
    "shortcutKeyMap": {
      "ShowWord": "Ctrl+K",
      "ToggleShowTranslate": "Ctrl+Z"
    }
  },
  "version": 23
}
```

- 动作名是上游的 `ShortcutKey` 枚举名（如 `ShowWord`），共 42 个。
- 缺少的条目、无法解析的条目、未知动作名一律回退到默认绑定，不会导致启动失败。
- 该字段不参与存档版本升级：新增字段带默认值即可，`setting.json` 的版本号仍是 23。

实现见 `src/domain/shortcut.rs`（数据模型与默认表）、`src/app/keymap.rs`（终端事件 → 键位）、`src/ui/shortcuts.rs`（编辑页）。

## 相关文档

- [使用手册](usage.md)
- [设置项参考](settings.md)
- [存档与数据格式](persistence.md)
- [与上游对照](upstream-parity.md)
