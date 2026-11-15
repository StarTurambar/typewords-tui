# 快捷键

本文是 typewords-tui 快捷键的权威参考：键位表模型、42 个动作与默认绑定、改绑方式、TUI 专有键位，以及与上游 TypeWords 的差异。页面级导航键与练习页操作见 [使用手册 · 按键参考](usage.md#按键参考)；设置入口见 [设置项参考](settings.md)。

## 1. 键位表模型

快捷键是**数据**而不是代码里的 `match` 分支：每个可配置动作（action）对应一个键位，整体存在 `setting.json` 的 `shortcutKeyMap` 字段里。

- 动作枚举：`src/domain/shortcut.rs` 的 `ShortcutAction`，与上游 `app/core/types/enum.ts` 的 `ShortcutKey` **一一对应（42 个）**。
- 键位表示：`ShortcutKey`（修饰键 `ctrl` / `alt` / `shift` + 基础键 `BaseKey`）。基础键不依赖终端 crate，领域层因此可以在不知道终端事件类型的情况下做匹配。
- 键位字符串格式与上游 `getShortcutKey` 一致：`Ctrl+` / `Alt+` / `Shift+` 前缀按此顺序拼接；方向键写作 `➡⬅⬆⬇`；空格写作 `Space`；ASCII 字母统一大写。界面展示时方向键换成更通用的窄箭头 `←→↑↓`。
- 查表：`ShortcutMap::actions_for` 把按键反查成动作列表。**同一个键位可以绑定多个动作**（例如 `1` 既是自测的「认识」也是选择题的 A），具体取哪一个由当前场景决定。
- 取消绑定：上游把 `shortcutKeyMap` 里某个动作的值写成空串表示「没有绑定」，本项目沿用同一约定（`ShortcutMap::unbind`）。未绑定的动作反查不会命中。

## 2. 改绑

1. 设置页 →「通用」分区 →「快捷键」→ `Enter`。
2. `↑` / `↓` 选择动作，`Enter` 开始捕获，敲下新键位即完成改绑；`Esc` 取消捕获。
3. `Delete` 清除该动作的绑定；`r` 恢复当前动作默认键位，`R` 恢复全部动作默认键位；`Esc` 返回设置页。

改绑会立即生效并写入 `setting.json`。若新键位已被别的动作占用，会提示「该键位已被「X」占用」并保持原绑定。终端兼容别名同样参与冲突检查：把某个动作绑到 `Ctrl+N` 会被拒绝，因为 `Ctrl+N` 已是「下一词」的别名。

## 3. 默认键位

下表与上游 `DefaultShortcutKeyMap` 逐条一致，除「差异」一节列出的 `ToggleConciseMode`（因终端限制改为 `Ctrl+G`）。

| 动作 | 说明 | 默认键位 | 状态 |
| --- | --- | --- | --- |
| EditArticle | 编辑文章 | `Ctrl+E` | 暂不可用 |
| PlayWordPronunciation | 播放单词发音 | `Ctrl+P` | 可用 |
| ToggleShowTranslate | 切换翻译 | `Ctrl+Z` | 可用 |
| ToggleDictation | 切换默写遮罩 | `Ctrl+I` | 可用 |
| ToggleTheme | 切换主题 | `Ctrl+Q` | 可用 |
| ToggleConciseMode | 切换简洁模式 | `Ctrl+G` | 可用 |
| ToggleToolbar | 切换工具栏 | `Ctrl+B` | 可用 |
| TogglePanel | 切换词表面板 | `Ctrl+L` | 可用 |
| RandomWrite | 随机默写 | `Ctrl+R` | 可用 |
| Previous / Next | 上一个词 / 下一个词 | `Ctrl+←` / `Ctrl+→` | 可用 |
| PreviousChapter / NextChapter | 上一组 / 下一组 | `Alt+←` / `Alt+→` | 可用 |
| ShowWord | 显示单词 | `Escape` | 可用 |
| Ignore | 跳过单词 | `Tab` | 可用 |
| ToggleSimple | 标记已掌握 | 反引号键 | 可用 |
| ToggleCollect | 收藏 | `Enter` | 可用 |
| CollectToDict | 收藏到指定词典 | `Shift+Enter` | 可用 |
| NextStep | 跳到下一阶段 | `Shift+→` | 可用 |
| RepeatChapter | 重学本组 | `Ctrl+Enter` | 可用 |
| DictationChapter | 默写本组 | `Alt+Enter` | 可用 |
| KnowWord / UnknownWord / MasteredWord | 自测：认识 / 不认识 / 已掌握 | `1` / `2` / `3` | 可用 |
| ChooseA~ChooseD | 选择题 A~D | `1` / `2` / `3` / `4` | 可用 |
| WordTestingNext | 单词测试：下一题 | `Space` | 可用 |
| SelfTestingChooseA~D | 自测选择题 A~D | `Alt+1` ~ `Alt+4` | 可用 |
| PlaySentence1..9 | 播放例句 1..9 | `Ctrl+1` … `Ctrl+9` | 暂不可用 |

「暂不可用」表示绑定仍然保留（便于与上游存档互通），但当前版本还没有对应功能，实现后会去掉标记。

### 生效范围

键位表全站共用，但每个页面只认自己有的动作；同一个功能在两个练习页用同一个键：

| 页面 | 认的动作 |
| --- | --- |
| 练习页（背单词） | 除 `EditArticle`、`ChooseA`~`ChooseD`、`WordTestingNext`、`PlaySentence1..9` 外的全部动作 |
| 文章跟打页 | `PlayWordPronunciation`（朗读当前句）、`ToggleShowTranslate`（切换当前句译文）、`ToggleTheme`；其余动作让位给跟打输入 |
| 单词测试页 | `ChooseA`~`ChooseD`、`WordTestingNext`、`PlayWordPronunciation` |
| 快捷键编辑页 | `Enter` 捕获、`Delete` 清除、`r` / `R` 恢复默认、`Esc` 返回 |

两个练习页另有共用的 TUI 专有键位（`Ctrl+C` / `Ctrl+,` / `?`），见第 4 节。两页唯一的键位差异是 `Esc`：练习页的它被上游绑成「显示单词」，跟打页没有遮罩，`Esc` 保持全站通用的「返回」。

### 动作按钮与键位表

两个练习页底部还有一排动作按钮：按钮上的键名直接读键位表，改绑后跟着变；没有绑定的按钮只显示动作名。按钮与键位执行同一段代码，所以「跳过」按钮与 `Tab` 等价。

`←` / `→` / `↑` / `↓` 在按钮条上用于选中按钮——它们不在键位表里，只在不带修饰键时生效。把动作绑到裸方向键上是允许的，且**键位表优先**：练习页先让键位表与键入消费按键，没人要的方向键才用来选中按钮。

### 终端兼容别名

部分终端会吞掉 `Ctrl+方向键`，另一些把 `Ctrl+J` 报成换行。因此额外提供两个**不落盘**的别名：

| 动作 | 别名 |
| --- | --- |
| Next（下一个词） | `Ctrl+N` |
| NextStep（跳到下一阶段） | `Ctrl+J` |

别名与主键位同时生效；用户改绑主键位不会影响别名。

## 4. TUI 专有键位

下列键位在上游没有对应动作，或终端形态不同，因此不走键位表：

| 键位 | 作用 | 说明 |
| --- | --- | --- |
| `Ctrl+C` | 保存并退出练习（两个练习页） | 终端保留键，不可改绑；练习页回主页、文章跟打页回文章列表 |
| `Ctrl+,` | 暂停 / 恢复计时（两个练习页） | 上游没有手动暂停，这是 TUI 专有键位 |
| `Ctrl+E` | 显示 / 隐藏例句与词源 | 上游该键位是「编辑文章」 |
| `?` | 打开帮助（两个练习页） | 会话 / 跟打进度保留，返回时回到对应页面 |
| `↑` / `↓` / `←` / `→`（`j` / `k` 为上下别名）、`Tab` / `Shift+Tab` | 标签页焦点导航 / 换栏 | 上游只有鼠标操作；流程页「下一节点」从 `Tab` 改为 `]` |
| `1` ~ `6`、`d` / `s` / `t`、`m`、`f`、`+` / `-`、`q` | 页面级导航与练习入口 | 见 [使用手册 · 按键参考](usage.md#按键参考) |
| `]`（流程页） | 跳到下一个节点 | 原 `Tab` 改为导航栏 ↔ 内容区换栏 |
| `/`（词条页） | 聚焦搜索框 | 再按一次或 `Esc` 退出 |

## 5. 与上游的差异

| 差异 | 上游 | TUI | 原因 |
| --- | --- | --- | --- |
| 简洁模式 | `Ctrl+M` | `Ctrl+G` | `Ctrl+M` 在多数终端等同回车，无法与 `Enter` 区分 |
| 练习内暂停计时 | 无（上游 `Ctrl+P` 是播放发音） | `Ctrl+,` | TUI 专有键位 |
| 例句 / 词源开关 | 无（上游 `Ctrl+E` 是编辑文章） | `Ctrl+E` | 文章编辑功能落地前借用该键位 |
| 下一个词 / 下一阶段 | `Ctrl+→` / `Shift+→` | 同左，另加 `Ctrl+N` / `Ctrl+J` 别名 | 部分终端不支持 `Ctrl+方向键` |
| 句子音频 | 逐句 mp3 时间轴 | 系统 TTS 朗读当前句 | 终端无音频资源；`PlaySentence1..9` 暂不可用 |

其余动作的默认键位、字符串格式与存档字段名都与上游一致，Web 端导出的 `shortcutKeyMap` 可以直接使用。

## 6. 存档格式

`setting.json` 的 `shortcutKeyMap` 字段，类型为 `Record<string, string>`（动作名 → 键位字符串），与上游 `SettingState.shortcutKeyMap` 同构：

    {
      "val": {
        "shortcutKeyMap": {
          "ShowWord": "Ctrl+K",
          "ToggleShowTranslate": "Ctrl+Z"
        }
      },
      "version": 23
    }

- 动作名是上游的 `ShortcutKey` 枚举名（如 `ShowWord`），共 42 个。
- 缺少的条目、无法解析的条目、未知动作名一律回退默认绑定，不会导致启动失败。
- 空串表示「已解除绑定」：该动作不再响应任何键位。
- 该字段不参与存档版本升级：新增字段带默认值即可，`setting.json` 的版本号仍是 23。

实现见 `src/domain/shortcut.rs`（数据模型与默认表）、`src/app/keymap.rs`（终端事件 → 键位）、`src/ui/shortcuts.rs`（编辑页）。

## 相关文档

- [使用手册 · 按键参考](usage.md#按键参考)
- [设置项参考](settings.md)
- [存档与数据格式](persistence.md)
- [与上游对照](upstream-parity.md)
- [总体架构](architecture.md)
