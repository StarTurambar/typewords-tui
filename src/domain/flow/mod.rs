//! 练习流程编排：内置 flow、游标、相位解析、阶段展示。

pub mod navigator;
pub mod store;

use serde::{Deserialize, Serialize};

use crate::domain::constants::{CURRENT_FLOW_VERSION, GROUP_SIZE};
use crate::domain::i18n::tr;
use crate::domain::types::{
    TaskWords, Word, WordPracticeMode, WordPracticeStage, WordPracticeType,
};

// ── 游标（需要持久化到练习缓存）────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeLoopRange {
    pub start_index: usize,
    pub end_index: usize,
    pub sub_step_index: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeFlowCursor {
    pub node_index: usize,
    pub step_index: usize,
    pub in_wrong_word_clear: bool,
    #[serde(rename = "loop")]
    pub loop_range: Option<PracticeLoopRange>,
    pub end_action_index: Option<usize>,
}

impl PracticeFlowCursor {
    pub fn at_start(&self) -> bool {
        self.node_index == 0
            && self.step_index == 0
            && !self.in_wrong_word_clear
            && self.loop_range.is_none()
    }
}

// ── 流程配置（内置，无需序列化）────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PracticeStepTemplateId {
    #[default]
    FollowWrite,
    Spell,
    Listen,
    Dictation,
    Identify,
}

impl PracticeStepTemplateId {
    pub fn practice_type(self) -> WordPracticeType {
        match self {
            Self::FollowWrite => WordPracticeType::FollowWrite,
            Self::Spell => WordPracticeType::Spell,
            Self::Listen => WordPracticeType::Listen,
            Self::Dictation => WordPracticeType::Dictation,
            Self::Identify => WordPracticeType::Identify,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FollowWrite => "followWrite",
            Self::Spell => "spell",
            Self::Listen => "listen",
            Self::Dictation => "dictation",
            Self::Identify => "identify",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PracticeWordsSource {
    #[default]
    TaskNew,
    TaskReview,
    Current,
    WrongWords,
}

/// 词内推进配置。
///
/// 字段名与上游 `PracticeWordAdvanceConfig` 一致（`groupSize` / `subSteps`），
/// 否则 `flows.json` 与上游的 `PracticeFlowV2` 互相读不了。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WordAdvance {
    #[default]
    Increment,
    WordLoop {
        group_size: usize,
        sub_steps: Vec<PracticeLoopSubStep>,
    },
}

impl WordAdvance {
    pub fn group_size(&self) -> usize {
        match self {
            Self::Increment => 1,
            Self::WordLoop { group_size, .. } => *group_size,
        }
    }

    pub fn sub_steps(&self) -> &[PracticeLoopSubStep] {
        match self {
            Self::Increment => &[],
            Self::WordLoop { sub_steps, .. } => sub_steps,
        }
    }
}

/// wordLoop 每组完成后的子练习步骤（上游 `PracticeLoopSubStep`）。
///
/// 上游是 `{ templateId, label? }` 对象，不是裸的模板 id——序列化必须按
/// 对象写，否则上游的 `isValidSubStep` 校验通不过。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeLoopSubStep {
    /// 子步骤用的模板
    pub template_id: PracticeStepTemplateId,
    /// 展示名（可选）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl From<PracticeStepTemplateId> for PracticeLoopSubStep {
    fn from(template_id: PracticeStepTemplateId) -> Self {
        Self {
            template_id,
            label: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollectTarget {
    Favorite,
    WrongBook,
}

/// 阶段结束后的动作队列（上游 `PracticeEndAction`）。
///
/// 上游把「跳转」并成一种类型、用 `target` 区分目标（`nextStep` / `complete`），
/// 「生成报告」也带一个 `reportType`。这里保持同样的结构，两端才能互读。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PracticeEndAction {
    WrongWordClear {
        template_id: PracticeStepTemplateId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word_advance: Option<WordAdvance>,
    },
    CollectWrongWords {
        target: CollectTarget,
    },
    GenerateReport {
        #[serde(default = "default_report_type")]
        report_type: String,
    },
    Navigate {
        target: NavigateTarget,
    },
}

/// 报告类型（上游 `stepSummary` / `sessionSummary`）。
fn default_report_type() -> String {
    "stepSummary".to_string()
}

/// 跳转目标（上游 `nextStep` / `complete`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NavigateTarget {
    #[default]
    NextStep,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeFlowStep {
    pub template_id: PracticeStepTemplateId,
    pub word_advance: WordAdvance,
    pub on_end: Vec<PracticeEndAction>,
    pub shuffle_on_enter: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeFlowNode {
    pub id: String,
    pub label: String,
    pub source: PracticeWordsSource,
    pub steps: Vec<PracticeFlowStep>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PracticeFlowConfig {
    pub id: String,
    pub version: u32,
    pub mode: WordPracticeMode,
    pub label: String,
    pub nodes: Vec<PracticeFlowNode>,
}

/// 解析后的相位（当前正在练什么）。
#[derive(Clone, Debug, PartialEq)]
pub struct PracticePhase {
    pub practice_type: WordPracticeType,
    pub word_advance: WordAdvance,
    pub on_end: Vec<PracticeEndAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlowStartResult {
    pub words: Vec<Word>,
    pub total: usize,
    pub new_word_number: usize,
    pub review_word_number: usize,
    pub cursor: PracticeFlowCursor,
}

fn step(
    template_id: PracticeStepTemplateId,
    word_advance: WordAdvance,
    on_end: Vec<PracticeEndAction>,
    shuffle_on_enter: bool,
) -> PracticeFlowStep {
    PracticeFlowStep {
        template_id,
        word_advance,
        on_end,
        shuffle_on_enter,
    }
}

fn word_loop_with_spell() -> WordAdvance {
    WordAdvance::WordLoop {
        group_size: GROUP_SIZE,
        sub_steps: vec![PracticeStepTemplateId::Spell.into()],
    }
}

fn default_on_end() -> Vec<PracticeEndAction> {
    vec![PracticeEndAction::WrongWordClear {
        template_id: PracticeStepTemplateId::FollowWrite,
        word_advance: Some(word_loop_with_spell()),
    }]
}

/// 7 个内置流程（与 Web 的 BUILTIN_FLOWS 一致）。
pub fn builtin_flow(id: &str) -> Option<PracticeFlowConfig> {
    let config = match id {
        "system" => PracticeFlowConfig {
            id: "system".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::System,
            label: "smart_learning".to_string(),
            nodes: vec![
                PracticeFlowNode {
                    id: "new".to_string(),
                    label: "new_words".to_string(),
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![
                        step(
                            PracticeStepTemplateId::FollowWrite,
                            word_loop_with_spell(),
                            default_on_end(),
                            false,
                        ),
                        step(
                            PracticeStepTemplateId::Listen,
                            WordAdvance::Increment,
                            default_on_end(),
                            true,
                        ),
                        step(
                            PracticeStepTemplateId::Dictation,
                            WordAdvance::Increment,
                            default_on_end(),
                            true,
                        ),
                    ],
                },
                PracticeFlowNode {
                    id: "review".to_string(),
                    label: "review".to_string(),
                    source: PracticeWordsSource::TaskReview,
                    steps: vec![
                        step(
                            PracticeStepTemplateId::Identify,
                            WordAdvance::Increment,
                            default_on_end(),
                            false,
                        ),
                        step(
                            PracticeStepTemplateId::Listen,
                            WordAdvance::Increment,
                            default_on_end(),
                            true,
                        ),
                        step(
                            PracticeStepTemplateId::Dictation,
                            WordAdvance::Increment,
                            default_on_end(),
                            true,
                        ),
                    ],
                },
            ],
        },
        "free" => PracticeFlowConfig {
            id: "free".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::Free,
            label: "free_practice".to_string(),
            nodes: vec![PracticeFlowNode {
                id: "practice".to_string(),
                label: "free_practice".to_string(),
                source: PracticeWordsSource::Current,
                steps: vec![step(
                    PracticeStepTemplateId::FollowWrite,
                    WordAdvance::Increment,
                    default_on_end(),
                    false,
                )],
            }],
        },
        "review" => PracticeFlowConfig {
            id: "review".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::Review,
            label: "review".to_string(),
            nodes: vec![PracticeFlowNode {
                id: "review".to_string(),
                label: "review".to_string(),
                source: PracticeWordsSource::TaskReview,
                steps: vec![
                    step(
                        PracticeStepTemplateId::Identify,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    ),
                    step(
                        PracticeStepTemplateId::Listen,
                        WordAdvance::Increment,
                        default_on_end(),
                        true,
                    ),
                    step(
                        PracticeStepTemplateId::Dictation,
                        WordAdvance::Increment,
                        default_on_end(),
                        true,
                    ),
                ],
            }],
        },
        "identifyOnly" => PracticeFlowConfig {
            id: "identifyOnly".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::IdentifyOnly,
            label: "identify".to_string(),
            nodes: vec![
                PracticeFlowNode {
                    id: "new".to_string(),
                    label: "new_words".to_string(),
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![step(
                        PracticeStepTemplateId::Identify,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    )],
                },
                PracticeFlowNode {
                    id: "review".to_string(),
                    label: "review".to_string(),
                    source: PracticeWordsSource::TaskReview,
                    steps: vec![step(
                        PracticeStepTemplateId::Identify,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    )],
                },
            ],
        },
        "dictationOnly" => PracticeFlowConfig {
            id: "dictationOnly".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::DictationOnly,
            label: "dictation".to_string(),
            nodes: vec![
                PracticeFlowNode {
                    id: "new".to_string(),
                    label: "new_words".to_string(),
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![step(
                        PracticeStepTemplateId::Dictation,
                        WordAdvance::Increment,
                        default_on_end(),
                        true,
                    )],
                },
                PracticeFlowNode {
                    id: "review".to_string(),
                    label: "review".to_string(),
                    source: PracticeWordsSource::TaskReview,
                    steps: vec![step(
                        PracticeStepTemplateId::Dictation,
                        WordAdvance::Increment,
                        default_on_end(),
                        true,
                    )],
                },
            ],
        },
        "listenOnly" => PracticeFlowConfig {
            id: "listenOnly".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::ListenOnly,
            label: "listen".to_string(),
            nodes: vec![
                PracticeFlowNode {
                    id: "new".to_string(),
                    label: "new_words".to_string(),
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![step(
                        PracticeStepTemplateId::Listen,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    )],
                },
                PracticeFlowNode {
                    id: "review".to_string(),
                    label: "review".to_string(),
                    source: PracticeWordsSource::TaskReview,
                    steps: vec![step(
                        PracticeStepTemplateId::Listen,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    )],
                },
            ],
        },
        "shuffle" => PracticeFlowConfig {
            id: "shuffle".to_string(),
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::Shuffle,
            label: "random_review".to_string(),
            nodes: vec![PracticeFlowNode {
                id: "practice".to_string(),
                label: "random_review".to_string(),
                source: PracticeWordsSource::TaskReview,
                steps: vec![step(
                    PracticeStepTemplateId::Dictation,
                    WordAdvance::Increment,
                    default_on_end(),
                    true,
                )],
            }],
        },
        _ => return None,
    };
    Some(config)
}

pub fn get_flow_config(id: &str) -> PracticeFlowConfig {
    builtin_flow(id).unwrap_or_else(|| builtin_flow("system").expect("system flow must exist"))
}

/// 流程配置是否可用（对应上游 `isValidFlowConfig`）。
///
/// 至少要有一个节点，且每个节点至少有一个步骤——否则游标无处落脚，
/// 取步骤与推进游标都会越界。自建流程来自 `flows.json`，可能被手工编辑、
/// 截断或从上游导入，因此必须在使用前校验。
pub fn is_valid_flow_config(config: &PracticeFlowConfig) -> bool {
    if config.nodes.is_empty() {
        return false;
    }
    config.nodes.iter().all(|node| !node.steps.is_empty())
}

/// 取一个保证可用的流程配置：非法配置回退到内置 system 流程。
///
/// 对应上游 `resolveFlowConfigOrSystem`。
pub fn resolve_flow_config_or_system(config: &PracticeFlowConfig) -> PracticeFlowConfig {
    if is_valid_flow_config(config) {
        config.clone()
    } else {
        get_flow_config("system")
    }
}

/// 按模式取内置流程 id（对齐上游 `getFlowIdForMode` 的 `?? 'system'` 兜底）。
///
/// 兜底只保证「总能拿到一个可跑的流程配置」，不代表该模式真的会执行「学习」
/// 流程：单词测试（`ShuffleWordsTest` / `ReviewWordsTest`）走 4 选 1 答题、
/// `Custom` 用已启用的自建流程，两者都没有内置 flow。判断「当前模式有没有
/// 流程会跑」用 `App::effective_flow_id`（单词测试与未启用自建流程的
/// `Custom` 返回 `None`），不要拿本函数的 `"system"` 当依据。
pub fn get_flow_id_for_mode(mode: WordPracticeMode) -> &'static str {
    match mode {
        WordPracticeMode::System => "system",
        WordPracticeMode::Free => "free",
        WordPracticeMode::IdentifyOnly => "identifyOnly",
        WordPracticeMode::DictationOnly => "dictationOnly",
        WordPracticeMode::ListenOnly => "listenOnly",
        WordPracticeMode::Shuffle => "shuffle",
        WordPracticeMode::Review => "review",
        _ => "system",
    }
}

/// 有内置流程的 7 个练习模式（与 `builtin_flow` 的 7 个 id 一一对应）。
///
/// 单词测试（`ShuffleWordsTest` / `ReviewWordsTest`）走 4 选 1 答题，`Custom`
/// 用已启用的自建流程，它们都没有内置 flow——流程页因此只列这 7 条，
/// 「启用」时也只会把这里对应的模式写回设置。
pub const BUILTIN_FLOW_MODES: [WordPracticeMode; 7] = [
    WordPracticeMode::System,
    WordPracticeMode::Free,
    WordPracticeMode::IdentifyOnly,
    WordPracticeMode::DictationOnly,
    WordPracticeMode::ListenOnly,
    WordPracticeMode::Shuffle,
    WordPracticeMode::Review,
];

/// 按模式取内置流程。
pub fn flow_for_mode(mode: WordPracticeMode) -> PracticeFlowConfig {
    get_flow_config(get_flow_id_for_mode(mode))
}

pub fn get_initial_cursor() -> PracticeFlowCursor {
    PracticeFlowCursor::default()
}

pub fn get_step<'a>(
    config: &'a PracticeFlowConfig,
    cursor: &PracticeFlowCursor,
) -> &'a PracticeFlowStep {
    config
        .nodes
        .get(cursor.node_index)
        .and_then(|node| node.steps.get(cursor.step_index))
        .or_else(|| {
            // 游标越界时退回第一个节点的首个步骤。
            config.nodes.first().and_then(|node| node.steps.first())
        })
        .unwrap_or(&EMPTY_STEP)
}

/// 流程配置不可用时的占位步骤。
///
/// `get_step` 的返回值不能为空引用，但配置本身可能已经损坏（自建流程来自
/// 可编辑的 `flows.json`）。此时返回一个「跟写 + 单步推进」的安全占位步骤，
/// 而不是越界 panic。
static EMPTY_STEP: PracticeFlowStep = PracticeFlowStep {
    template_id: PracticeStepTemplateId::FollowWrite,
    word_advance: WordAdvance::Increment,
    on_end: Vec::new(),
    shuffle_on_enter: false,
};

pub fn materialize_step(step: &PracticeFlowStep) -> PracticePhase {
    PracticePhase {
        practice_type: step.template_id.practice_type(),
        word_advance: step.word_advance.clone(),
        on_end: step.on_end.clone(),
    }
}

pub fn resolve_phase_by_cursor(
    config: &PracticeFlowConfig,
    cursor: &PracticeFlowCursor,
) -> PracticePhase {
    let main_phase = materialize_step(get_step(config, cursor));
    let mut phase = main_phase.clone();

    if cursor.in_wrong_word_clear {
        let index = cursor.end_action_index.unwrap_or(0);
        if let Some(PracticeEndAction::WrongWordClear {
            template_id,
            word_advance,
        }) = main_phase.on_end.get(index)
        {
            phase = PracticePhase {
                practice_type: template_id.practice_type(),
                word_advance: word_advance.clone().unwrap_or_default(),
                on_end: Vec::new(),
            };
        }
    }

    if let Some(loop_range) = &cursor.loop_range
        && let Some(sub_step) = phase
            .word_advance
            .sub_steps()
            .get(loop_range.sub_step_index)
    {
        let mut next = PracticePhase {
            practice_type: sub_step.template_id.practice_type(),
            word_advance: phase.word_advance.clone(),
            on_end: Vec::new(),
        };
        // 子步骤沿用主相位的 on_end（收尾仍由所属 Step 决定）
        next.on_end = phase.on_end.clone();
        return next;
    }

    phase
}

/// 当前相位实际生效的练习方式：错词清空与组内巩固子步骤都会覆盖主步骤的方式。
fn effective_template(
    config: &PracticeFlowConfig,
    cursor: &PracticeFlowCursor,
) -> PracticeStepTemplateId {
    let step = get_step(config, cursor);
    if cursor.in_wrong_word_clear {
        let index = cursor.end_action_index.unwrap_or(0);
        if let Some(PracticeEndAction::WrongWordClear { template_id, .. }) = step.on_end.get(index)
        {
            return *template_id;
        }
    }
    if let Some(loop_range) = &cursor.loop_range
        && let Some(sub_step) = step.word_advance.sub_steps().get(loop_range.sub_step_index)
    {
        return sub_step.template_id;
    }
    step.template_id
}

/// 由游标推出上游的 `WordPracticeStage`（存档里的 `statStoreData.stage`）。
///
/// 上游只区分「新词 / 复习」两类词表与四种练习方式；`Spell` 是跟写阶段内的
/// 组内巩固子步骤，归回跟写阶段。随机复习模式在上游只有 `Shuffle` 一个阶段。
pub fn stage_for_cursor(
    config: &PracticeFlowConfig,
    cursor: &PracticeFlowCursor,
) -> WordPracticeStage {
    if config.mode == WordPracticeMode::Shuffle {
        return WordPracticeStage::Shuffle;
    }
    let source = config
        .nodes
        .get(cursor.node_index)
        .map(|node| node.source)
        .unwrap_or(PracticeWordsSource::TaskNew);
    let template = effective_template(config, cursor);
    match (source, template) {
        (PracticeWordsSource::TaskNew, PracticeStepTemplateId::FollowWrite)
        | (PracticeWordsSource::TaskNew, PracticeStepTemplateId::Spell)
        | (PracticeWordsSource::Current, _) => WordPracticeStage::FollowWriteNewWord,
        (PracticeWordsSource::TaskNew, PracticeStepTemplateId::Identify) => {
            WordPracticeStage::IdentifyNewWord
        }
        (PracticeWordsSource::TaskNew, PracticeStepTemplateId::Listen) => {
            WordPracticeStage::ListenNewWord
        }
        (PracticeWordsSource::TaskNew, PracticeStepTemplateId::Dictation) => {
            WordPracticeStage::DictationNewWord
        }
        (PracticeWordsSource::TaskReview, PracticeStepTemplateId::FollowWrite)
        | (PracticeWordsSource::TaskReview, PracticeStepTemplateId::Spell) => {
            WordPracticeStage::FollowWriteReview
        }
        (PracticeWordsSource::TaskReview, PracticeStepTemplateId::Identify) => {
            WordPracticeStage::IdentifyReview
        }
        (PracticeWordsSource::TaskReview, PracticeStepTemplateId::Listen) => {
            WordPracticeStage::ListenReview
        }
        (PracticeWordsSource::TaskReview, PracticeStepTemplateId::Dictation) => {
            WordPracticeStage::DictationReview
        }
        (PracticeWordsSource::WrongWords, _) => WordPracticeStage::FollowWriteNewWord,
    }
}

pub fn advance_step_cursor(
    config: &PracticeFlowConfig,
    cursor: &PracticeFlowCursor,
) -> (PracticeFlowCursor, bool) {
    // 空节点列表（配置损坏）没有可推进的步骤，直接视为已完成。
    if config.nodes.is_empty() {
        return (cursor.clone(), true);
    }
    let node = &config.nodes[cursor.node_index.min(config.nodes.len() - 1)];
    let is_last_step = cursor.step_index >= node.steps.len().saturating_sub(1);
    let is_last_node = cursor.node_index >= config.nodes.len().saturating_sub(1);
    if is_last_step && is_last_node {
        return (cursor.clone(), true);
    }
    let next = PracticeFlowCursor {
        node_index: if is_last_step {
            cursor.node_index + 1
        } else {
            cursor.node_index
        },
        step_index: if is_last_step {
            0
        } else {
            cursor.step_index + 1
        },
        in_wrong_word_clear: false,
        loop_range: None,
        end_action_index: None,
    };
    (next, false)
}

fn resolve_initial_words(
    source: PracticeWordsSource,
    task_words: &TaskWords,
    wrong_words: &[Word],
) -> Vec<Word> {
    match source {
        PracticeWordsSource::TaskNew => task_words.new.clone(),
        PracticeWordsSource::TaskReview => task_words.review.clone(),
        PracticeWordsSource::Current => {
            let mut words = task_words.new.clone();
            words.extend(task_words.review.iter().cloned());
            words
        }
        PracticeWordsSource::WrongWords => wrong_words.to_vec(),
    }
}

fn resolve_task_counts(
    config: &PracticeFlowConfig,
    task_words: &TaskWords,
) -> (usize, usize, usize) {
    let includes_new = config.nodes.iter().any(|node| {
        matches!(
            node.source,
            PracticeWordsSource::TaskNew | PracticeWordsSource::Current
        )
    });
    let includes_review = config.nodes.iter().any(|node| {
        matches!(
            node.source,
            PracticeWordsSource::TaskReview | PracticeWordsSource::Current
        )
    });
    let new_count = if includes_new {
        task_words.new.len()
    } else {
        0
    };
    let review_count = if includes_review {
        task_words.review.len()
    } else {
        0
    };
    (new_count + review_count, new_count, review_count)
}

/// 起点词表解析 + 游标初始化（首次进入会话）。
pub fn resolve_flow_start(
    config: &PracticeFlowConfig,
    task_words: &TaskWords,
    wrong_words: &[Word],
    shuffle_words: &mut dyn FnMut(&mut [Word]),
) -> Option<FlowStartResult> {
    if task_words.total() == 0 {
        return None;
    }
    for (index, node) in config.nodes.iter().enumerate() {
        let mut words = resolve_initial_words(node.source, task_words, wrong_words);
        if words.is_empty() {
            continue;
        }
        if node
            .steps
            .first()
            .map(|step| step.shuffle_on_enter)
            .unwrap_or(false)
        {
            shuffle_words(&mut words);
        }
        let (total, new_word_number, review_word_number) = resolve_task_counts(config, task_words);
        return Some(FlowStartResult {
            words,
            total,
            new_word_number,
            review_word_number,
            cursor: PracticeFlowCursor {
                node_index: index,
                ..PracticeFlowCursor::default()
            },
        });
    }
    None
}

// ── 阶段展示模型 ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct FlowProgressStage {
    pub name: String,
    pub ratio: u32,
    pub percentage: u32,
    pub active: bool,
    pub children: Vec<FlowProgressStage>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlowDisplayState {
    pub status: String,
    pub stages: Vec<FlowProgressStage>,
    pub show_skip_step: bool,
}

pub fn display_state(
    config: &PracticeFlowConfig,
    cursor: &PracticeFlowCursor,
    word_index: usize,
    word_count: usize,
) -> FlowDisplayState {
    let nodes = &config.nodes;
    let current_progress = if word_count > 0 {
        ((word_index as f64 / word_count as f64) * 100.0).round() as u32
    } else {
        0
    };

    let mut status = tr(&config.label);
    if cursor.loop_range.is_some() {
        status = tr("step_group");
    } else if cursor.in_wrong_word_clear {
        status = tr("review_wrong_words");
    } else if let Some(node) = nodes.get(cursor.node_index)
        && !(nodes.len() == 1 && node.steps.len() == 1)
    {
        let step_label = node
            .steps
            .get(cursor.step_index)
            .map(|step| tr(step.template_id.label()))
            .unwrap_or_default();
        status = if step_label.is_empty() {
            tr(&node.label)
        } else {
            format!("{} · {}", tr(&node.label), step_label)
        };
    }

    if nodes.is_empty() {
        return FlowDisplayState {
            status,
            stages: Vec::new(),
            show_skip_step: false,
        };
    }

    if nodes.len() == 1 && nodes[0].steps.len() == 1 {
        return FlowDisplayState {
            status,
            stages: vec![FlowProgressStage {
                name: String::new(),
                ratio: 100,
                percentage: current_progress,
                active: true,
                children: Vec::new(),
            }],
            show_skip_step: false,
        };
    }

    let is_single_node = nodes.len() == 1;
    let stages = nodes
        .iter()
        .enumerate()
        .map(|(node_index, node)| {
            let is_current = node_index == cursor.node_index;
            let is_completed = node_index < cursor.node_index;
            let children = if is_current && node.steps.len() > 1 {
                node.steps
                    .iter()
                    .enumerate()
                    .map(|(step_index, step)| FlowProgressStage {
                        name: tr(step.template_id.label()),
                        ratio: 100 / node.steps.len().max(1) as u32,
                        percentage: if step_index < cursor.step_index {
                            100
                        } else if step_index == cursor.step_index {
                            current_progress
                        } else {
                            0
                        },
                        active: step_index == cursor.step_index,
                        children: Vec::new(),
                    })
                    .collect()
            } else {
                Vec::new()
            };
            FlowProgressStage {
                name: tr(&node.label),
                ratio: if is_single_node {
                    100
                } else if is_current {
                    70
                } else {
                    30
                },
                percentage: if is_completed {
                    100
                } else if is_current {
                    current_progress
                } else {
                    0
                },
                active: is_current,
                children,
            }
        })
        .collect();

    FlowDisplayState {
        status,
        stages,
        show_skip_step: nodes.len() > 1 || nodes[0].steps.len() > 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 流程页只列 BUILTIN_FLOW_MODES 这 7 个模式，它们必须与内置流程表一一对应。
    ///
    /// 对不上时流程页会列出没有 flow 的模式（早期把两个单词测试模式也列了进去，
    /// 它们的配置退化成 system，启用时写回的是 system 模式）。
    #[test]
    fn builtin_flow_modes_match_the_builtin_flow_table() {
        assert_eq!(BUILTIN_FLOW_MODES.len(), 7);
        let mut ids: Vec<String> = Vec::new();
        for mode in BUILTIN_FLOW_MODES {
            let config = flow_for_mode(mode);
            assert_eq!(
                config.mode, mode,
                "内置流程 {:?} 配置里的 mode 应与来源模式一致",
                mode
            );
            assert_eq!(get_flow_id_for_mode(mode), config.id);
            assert!(!ids.contains(&config.id), "内置流程 id 重复: {}", config.id);
            ids.push(config.id);
        }
    }

    /// 单词测试与「自定义流程」没有内置流程：`get_flow_id_for_mode` 的
    /// `system` 只是 `&'static str` 的兜底，保证会话层总能拿到一份配置。
    ///
    /// 兜底不等于「该模式会跑「学习」流程」——单词测试在 `start_practice_with_mode`
    /// 里直接分流到答题页。这里用测试钉住兜底行为：谁要是以为它是纯清理、
    /// 顺手把返回值改成 `Option`，得先处理会话层与上游对齐结论。
    #[test]
    fn modes_without_a_builtin_flow_fall_back_to_system() {
        for mode in [
            WordPracticeMode::ShuffleWordsTest,
            WordPracticeMode::ReviewWordsTest,
            WordPracticeMode::Custom,
        ] {
            assert!(
                !BUILTIN_FLOW_MODES.contains(&mode),
                "{mode:?} 不该出现在内置流程列表里"
            );
            assert_eq!(get_flow_id_for_mode(mode), "system");
            assert_eq!(flow_for_mode(mode).id, "system");
        }
    }
}
