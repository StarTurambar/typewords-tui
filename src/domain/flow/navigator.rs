//! 练习推进器：对应 Web 的 usePracticeWordNavigator。
//!
//! - `cursor` 是唯一位置指针
//! - `resolve_phase_by_cursor` 是唯一相位查询入口
//! - `node_working_words` 是同一 Node 内 Step 之间传递的工作词表
//! - `on_end` 队列负责错词清空 / 即时动作 / 跳转

use crate::domain::flow::{
    PracticeEndAction, PracticeFlowConfig, PracticeFlowCursor, PracticeLoopRange, PracticePhase,
    PracticeWordsSource, WordAdvance, advance_step_cursor, get_flow_config, get_initial_cursor,
    get_step, materialize_step, resolve_phase_by_cursor,
};
use crate::domain::rng::{Rng, shuffle};
use crate::domain::types::{PracticeData, PracticeSessionSnapshot, PracticeState, TaskWords, Word};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeLevel {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
}

impl Notice {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Info,
            message: message.into(),
        }
    }
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Warning,
            message: message.into(),
        }
    }
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Success,
            message: message.into(),
        }
    }
}

/// 推进器运行所需的上下文（由会话层按字段借用拆开）。
pub struct NavState<'a> {
    pub data: &'a mut PracticeData,
    pub task_words: &'a TaskWords,
    pub stat: &'a mut PracticeState,
    pub is_simple: &'a dyn Fn(&str) -> bool,
    pub rng: &'a mut Rng,
    pub notices: &'a mut Vec<Notice>,
    /// 需要重置当前单词键入状态（对应 Web 的 EventKey.resetWord）。
    pub reset_word: &'a mut bool,
    /// 整个会话已完成。
    pub completed: &'a mut bool,
}

fn need_next(word: &Word, is_simple: &dyn Fn(&str) -> bool, exclude_words: &[String]) -> bool {
    if word.word.is_empty() {
        return false;
    }
    is_simple(&word.word) || exclude_words.iter().any(|key| key == &word.word)
}

fn filter_need_next(
    list: &mut Vec<Word>,
    is_simple: &dyn Fn(&str) -> bool,
    exclude_words: &[String],
) {
    list.retain(|word| !need_next(word, is_simple, exclude_words));
}

pub struct Navigator {
    pub flow: PracticeFlowConfig,
    pub cursor: PracticeFlowCursor,
    node_working_words: Vec<Word>,
}

impl Navigator {
    pub fn new(flow: PracticeFlowConfig) -> Self {
        Self {
            flow,
            cursor: get_initial_cursor(),
            node_working_words: Vec::new(),
        }
    }

    /// 重置游标与节点词表，**保留当前流程**。
    pub fn reset(&mut self) {
        self.cursor = get_initial_cursor();
        self.node_working_words.clear();
    }

    pub fn load_flow(&mut self, flow_id: &str) {
        self.flow = get_flow_config(flow_id);
    }

    pub fn active_flow_id(&self) -> &str {
        &self.flow.id
    }

    pub fn current_phase(&self) -> PracticePhase {
        resolve_phase_by_cursor(&self.flow, &self.cursor)
    }

    pub fn initialize_node_words(&mut self, words: Vec<Word>) {
        self.node_working_words = words;
    }

    pub fn node_working_words(&self) -> &[Word] {
        &self.node_working_words
    }

    pub fn set_cursor(&mut self, cursor: PracticeFlowCursor) {
        self.cursor = cursor;
    }

    fn source_for_cursor(&self) -> PracticeWordsSource {
        self.flow
            .nodes
            .get(self.cursor.node_index)
            .map(|node| node.source)
            .unwrap_or(PracticeWordsSource::Current)
    }

    fn current_word(&self, state: &NavState) -> Word {
        state.data.current_word_or_default()
    }

    fn at_list_end(&self, data: &PracticeData) -> bool {
        if let Some(loop_range) = &self.cursor.loop_range {
            return data.index >= loop_range.end_index;
        }
        data.words.is_empty() || data.index >= data.words.len().saturating_sub(1)
    }

    // ── 词表准备 ─────────────────────────────────────────────────────────

    fn filter_working_words(&mut self, state: &NavState) {
        let exclude = &state.data.exclude_words;
        let is_simple = state.is_simple;
        filter_need_next(&mut self.node_working_words, is_simple, exclude);
    }

    fn go_to_cursor(
        &mut self,
        state: &mut NavState,
        new_cursor: PracticeFlowCursor,
        origin_list: Vec<Word>,
        reset_node_words: bool,
        shuffle_words: bool,
    ) {
        let mut list = origin_list;
        filter_need_next(&mut list, state.is_simple, &state.data.exclude_words);
        if reset_node_words {
            self.node_working_words = list.clone();
        }
        if shuffle_words {
            shuffle(state.rng, &mut list);
        }
        self.cursor = new_cursor;
        state.data.words = list;
        state.data.index = 0;
    }

    fn resolve_words_from_source(
        &self,
        source: PracticeWordsSource,
        state: &NavState,
    ) -> Vec<Word> {
        match source {
            PracticeWordsSource::TaskNew => state.task_words.new.clone(),
            PracticeWordsSource::TaskReview => state.task_words.review.clone(),
            PracticeWordsSource::WrongWords => state.data.wrong_words.clone(),
            PracticeWordsSource::Current => state.data.words.clone(),
        }
    }

    /// 进入下一静态 Step；同 Node 继承工作词表，跨 Node 才重新解析 source。
    fn run_step_advance(&mut self, state: &mut NavState) -> bool {
        self.filter_working_words(state);

        loop {
            let current = self.cursor.clone();
            let (next_cursor, complete) = advance_step_cursor(&self.flow, &current);
            if complete {
                *state.completed = true;
                return true;
            }

            let changed_node = next_cursor.node_index != current.node_index;
            let next_source = self
                .flow
                .nodes
                .get(next_cursor.node_index)
                .map(|node| node.source)
                .unwrap_or(PracticeWordsSource::Current);
            let words = if changed_node {
                if next_source == PracticeWordsSource::Current {
                    self.node_working_words.clone()
                } else {
                    self.resolve_words_from_source(next_source, state)
                }
            } else {
                self.node_working_words.clone()
            };
            let mut list = words;
            filter_need_next(&mut list, state.is_simple, &state.data.exclude_words);

            let shuffle_on_enter = self
                .flow
                .nodes
                .get(next_cursor.node_index)
                .and_then(|node| node.steps.get(next_cursor.step_index))
                .map(|step| step.shuffle_on_enter)
                .unwrap_or(false);

            if list.is_empty() {
                self.cursor = next_cursor;
                state.data.words = Vec::new();
                state.data.index = 0;
                if changed_node {
                    self.node_working_words = Vec::new();
                }
                continue;
            }

            self.go_to_cursor(state, next_cursor, list, changed_node, shuffle_on_enter);
            return false;
        }
    }

    // ── wordLoop 子步骤 ──────────────────────────────────────────────────

    fn enter_loop(
        &mut self,
        state: &mut NavState,
        start_index: usize,
        end_index: usize,
        sub_step_index: usize,
    ) {
        let previous_word = self.current_word(state);
        self.cursor.loop_range = Some(PracticeLoopRange {
            start_index,
            end_index,
            sub_step_index,
        });
        state.data.index = start_index;
        let target_word = self.current_word(state);
        if target_word.word == previous_word.word {
            *state.reset_word = true;
        }
    }

    fn leave_loop(&mut self, state: &mut NavState, end_index: usize) {
        self.cursor.loop_range = None;
        state.data.index = end_index + 1;
    }

    fn advance_loop_sub_step(&mut self, state: &mut NavState, phase: &PracticePhase) {
        let Some(loop_range) = self.cursor.loop_range.clone() else {
            return;
        };
        let next_sub_step = loop_range.sub_step_index + 1;
        if next_sub_step < phase.word_advance.sub_steps().len() {
            self.enter_loop(
                state,
                loop_range.start_index,
                loop_range.end_index,
                next_sub_step,
            );
        } else {
            self.leave_loop(state, loop_range.end_index);
        }
    }

    fn run_word_loop(&mut self, state: &mut NavState, phase: &PracticePhase, group_size: usize) {
        let group_size = group_size.max(1);
        if self.cursor.loop_range.is_some() {
            state.data.index += 1;
            return;
        }
        state.data.index += 1;
        if state.data.index.is_multiple_of(group_size) && !phase.word_advance.sub_steps().is_empty()
        {
            let group_start = state.data.index - group_size;
            let group_end = state.data.index - 1;
            self.enter_loop(state, group_start, group_end, 0);
        }
    }

    fn run_word_advance(&mut self, state: &mut NavState, phase: &PracticePhase) {
        if matches!(phase.word_advance, WordAdvance::WordLoop { .. }) {
            self.run_word_loop(state, phase, phase.word_advance.group_size());
        } else {
            state.data.index += 1;
        }
    }

    // ── 错词清空 ─────────────────────────────────────────────────────────

    fn run_wrong_word_retry(&mut self, state: &mut NavState, notice: bool) {
        if notice {
            state
                .notices
                .push(Notice::info(crate::domain::i18n::tr("wrong_words_remain")));
        }
        let mut words = std::mem::take(&mut state.data.wrong_words);
        shuffle(state.rng, &mut words);
        state.data.words = words;
        state.data.index = 0;
        state.data.wrong_words = Vec::new();
    }

    fn execute_instant_action(&mut self, state: &mut NavState, action: &PracticeEndAction) {
        match action {
            PracticeEndAction::CollectWrongWords { target } => {
                // 结算阶段统一处理，这里只提示（与 Web 的 Phase 3+ 占位一致）。
                state.notices.push(Notice::info(format!(
                    "收藏错词 → {}",
                    match target {
                        crate::domain::flow::CollectTarget::Favorite => "收藏",
                        crate::domain::flow::CollectTarget::WrongBook => "错词本",
                    }
                )));
            }
            PracticeEndAction::GenerateReport => {}
            _ => {}
        }
    }

    fn process_next_end_action(
        &mut self,
        state: &mut NavState,
        phase: &PracticePhase,
        start_action_index: usize,
    ) -> bool {
        let mut action_index = start_action_index;
        while action_index < phase.on_end.len() {
            let action = phase.on_end[action_index].clone();
            match action {
                PracticeEndAction::WrongWordClear { .. } => {
                    let exclude = state.data.exclude_words.clone();
                    let is_simple = state.is_simple;
                    filter_need_next(&mut state.data.wrong_words, is_simple, &exclude);
                    if !state.data.wrong_words.is_empty() {
                        self.cursor.in_wrong_word_clear = true;
                        self.cursor.end_action_index = Some(action_index);
                        self.cursor.loop_range = None;
                        self.run_wrong_word_retry(state, true);
                        return false;
                    }
                    action_index += 1;
                }
                PracticeEndAction::NavigateComplete => {
                    *state.completed = true;
                    return true;
                }
                PracticeEndAction::NavigateNextStep => break,
                other => {
                    self.execute_instant_action(state, &other);
                    action_index += 1;
                }
            }
        }
        self.run_step_advance(state)
    }

    fn handle_list_end(
        &mut self,
        state: &mut NavState,
        phase: &PracticePhase,
        ignore_loop: bool,
    ) -> bool {
        let mut owner_phase = phase.clone();

        if !ignore_loop && self.cursor.loop_range.is_some() {
            let loop_end_index = self
                .cursor
                .loop_range
                .as_ref()
                .map(|range| range.end_index)
                .unwrap_or(0);
            self.advance_loop_sub_step(state, phase);
            if self.cursor.loop_range.is_some()
                || loop_end_index < state.data.words.len().saturating_sub(1)
            {
                return false;
            }
            owner_phase = self.current_phase();
        } else if ignore_loop && self.cursor.loop_range.is_some() {
            self.cursor.loop_range = None;
            owner_phase = self.current_phase();
        }

        // 情形 1：wordLoop 主阶段词表自然结束，最后一组尚未进入 loop 子步骤
        if !ignore_loop
            && matches!(owner_phase.word_advance, WordAdvance::WordLoop { .. })
            && !state.data.words.is_empty()
            && state.data.index < state.data.words.len()
        {
            let group_size = owner_phase.word_advance.group_size().max(1);
            let end_index = state.data.index;
            let group_start = (end_index / group_size) * group_size;
            if !owner_phase.word_advance.sub_steps().is_empty() {
                self.enter_loop(state, group_start, end_index, 0);
                return false;
            }
        }

        // 情形 2：处于错词清空（自身 loop 已完成，判断是否还有错词）
        if self.cursor.in_wrong_word_clear {
            let exclude = state.data.exclude_words.clone();
            let is_simple = state.is_simple;
            filter_need_next(&mut state.data.wrong_words, is_simple, &exclude);
            let main_phase = materialize_step(get_step(&self.flow, &self.cursor));
            if !state.data.wrong_words.is_empty() {
                let end_action_index = self.cursor.end_action_index.unwrap_or(0);
                if let Some(PracticeEndAction::WrongWordClear { .. }) =
                    main_phase.on_end.get(end_action_index)
                {
                    self.cursor.loop_range = None;
                    self.run_wrong_word_retry(state, false);
                    return false;
                }
            }
            let next_action_index = self.cursor.end_action_index.unwrap_or(0) + 1;
            self.cursor.in_wrong_word_clear = false;
            self.cursor.end_action_index = None;
            return self.process_next_end_action(state, &main_phase, next_action_index);
        }

        // 情形 3：普通主步骤词表练完
        self.filter_working_words(state);
        if !owner_phase.on_end.is_empty() {
            return self.process_next_end_action(state, &owner_phase, 0);
        }
        self.run_step_advance(state)
    }

    // ── 对外推进接口 ─────────────────────────────────────────────────────

    pub fn next(&mut self, state: &mut NavState, is_typing: bool, ignore_loop: bool) {
        let mut count_as_typing = is_typing;
        let mut should_ignore_loop = ignore_loop;

        loop {
            let word = self.current_word(state);
            if word.word.is_empty() {
                self.run_step_advance(state);
                return;
            }

            let key = word.word.clone();
            let pre_times = state.data.wrong_times_map.get(&key).copied().unwrap_or(0);
            let phase = self.current_phase();
            state
                .data
                .wrong_times_map
                .insert(key, pre_times + state.data.wrong_times);
            state.data.wrong_times = 0;
            if count_as_typing {
                state.stat.input_word_number += 1;
            }

            let completed = if self.at_list_end(state.data) {
                self.handle_list_end(state, &phase, should_ignore_loop)
            } else {
                self.run_word_advance(state, &phase);
                false
            };

            if completed {
                return;
            }
            let current = self.current_word(state);
            if state.data.words.is_empty()
                || !need_next(&current, state.is_simple, &state.data.exclude_words)
            {
                return;
            }

            count_as_typing = false;
            should_ignore_loop = false;
        }
    }

    /// 返回 false 表示已经在第一个单词。
    pub fn prev(&mut self, state: &mut NavState) -> bool {
        if state.data.index == 0 {
            state.notices.push(Notice::warning(crate::domain::i18n::tr(
                "already_first_word",
            )));
            return false;
        }
        state.data.index -= 1;
        true
    }

    pub fn skip_step(&mut self, state: &mut NavState) {
        state.data.index = state.data.words.len().saturating_sub(1);
        state.data.wrong_words.clear();
        self.next(state, false, true);
    }

    /// 外部批量交互已消费完当前词表，按当前 Phase 的 onEnd 队列正常收尾。
    pub fn complete_current_list(&mut self, state: &mut NavState) {
        let phase = self.current_phase();
        self.handle_list_end(state, &phase, true);
    }

    // ── 会话快照 ─────────────────────────────────────────────────────────

    pub fn build_snapshot(&self) -> PracticeSessionSnapshot {
        PracticeSessionSnapshot {
            flow_id: self.flow.id.clone(),
            cursor: self.cursor.clone(),
            node_working_word_keys: Some(
                self.node_working_words
                    .iter()
                    .map(|word| word.word.clone())
                    .collect(),
            ),
        }
    }

    fn restore_cursor(&mut self, cursor: &PracticeFlowCursor, data: &PracticeData) -> bool {
        let node = self.flow.nodes.get(cursor.node_index);
        if node.is_none()
            || node
                .and_then(|node| node.steps.get(cursor.step_index))
                .is_none()
        {
            self.cursor = get_initial_cursor();
            return false;
        }

        let main_phase = resolve_phase_by_cursor(
            &self.flow,
            &PracticeFlowCursor {
                node_index: cursor.node_index,
                step_index: cursor.step_index,
                in_wrong_word_clear: false,
                loop_range: None,
                end_action_index: None,
            },
        );
        if cursor.in_wrong_word_clear
            && !matches!(
                main_phase.on_end.get(cursor.end_action_index.unwrap_or(0)),
                Some(PracticeEndAction::WrongWordClear { .. })
            )
        {
            self.cursor = get_initial_cursor();
            return false;
        }

        if let Some(loop_range) = &cursor.loop_range {
            let phase_without_loop = resolve_phase_by_cursor(
                &self.flow,
                &PracticeFlowCursor {
                    loop_range: None,
                    ..cursor.clone()
                },
            );
            let valid = matches!(
                phase_without_loop.word_advance,
                WordAdvance::WordLoop { .. }
            ) && loop_range.sub_step_index
                < phase_without_loop.word_advance.sub_steps().len()
                && loop_range.end_index >= loop_range.start_index
                && loop_range.end_index < data.words.len();
            if !valid {
                self.cursor = get_initial_cursor();
                return false;
            }
        }

        self.cursor = cursor.clone();
        true
    }

    fn restore_working_words(
        &mut self,
        keys: Option<&Vec<String>>,
        task_words: &TaskWords,
        data: &PracticeData,
    ) {
        if let Some(keys) = keys {
            let mut map: std::collections::HashMap<&str, &Word> = std::collections::HashMap::new();
            for word in task_words
                .new
                .iter()
                .chain(task_words.review.iter())
                .chain(data.words.iter())
                .chain(data.wrong_words.iter())
            {
                map.insert(word.word.as_str(), word);
            }
            self.node_working_words = keys
                .iter()
                .filter_map(|key| map.get(key.as_str()).map(|word| (*word).clone()))
                .collect();
            return;
        }
        self.node_working_words = match self.source_for_cursor_after_restore(task_words, data) {
            PracticeWordsSource::TaskNew => task_words.new.clone(),
            PracticeWordsSource::TaskReview => task_words.review.clone(),
            PracticeWordsSource::WrongWords => data.wrong_words.clone(),
            PracticeWordsSource::Current => data.words.clone(),
        };
    }

    fn source_for_cursor_after_restore(
        &self,
        _task_words: &TaskWords,
        _data: &PracticeData,
    ) -> PracticeWordsSource {
        self.source_for_cursor()
    }

    pub fn restore_snapshot(
        &mut self,
        snapshot: &PracticeSessionSnapshot,
        task_words: &TaskWords,
        data: &PracticeData,
    ) -> bool {
        self.flow = get_flow_config(&snapshot.flow_id);
        let flow_matched = self.flow.id == snapshot.flow_id;
        let cursor_restored = flow_matched && self.restore_cursor(&snapshot.cursor, data);
        if !cursor_restored {
            self.flow = get_flow_config("system");
            self.cursor = get_initial_cursor();
        }
        self.restore_working_words(
            if cursor_restored {
                snapshot.node_working_word_keys.as_ref()
            } else {
                None
            },
            task_words,
            data,
        );
        cursor_restored
    }
}
