//! 练习流程编排：内置 flow、游标、相位解析、阶段展示。

pub mod navigator;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticeStepTemplateId {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticeWordsSource {
    TaskNew,
    TaskReview,
    Current,
    WrongWords,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WordAdvance {
    Increment,
    WordLoop {
        group_size: usize,
        sub_steps: Vec<PracticeStepTemplateId>,
    },
}

impl WordAdvance {
    pub fn group_size(&self) -> usize {
        match self {
            Self::Increment => 1,
            Self::WordLoop { group_size, .. } => *group_size,
        }
    }

    pub fn sub_steps(&self) -> &[PracticeStepTemplateId] {
        match self {
            Self::Increment => &[],
            Self::WordLoop { sub_steps, .. } => sub_steps,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CollectTarget {
    Favorite,
    WrongBook,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PracticeEndAction {
    WrongWordClear {
        template_id: PracticeStepTemplateId,
        word_advance: WordAdvance,
    },
    CollectWrongWords {
        target: CollectTarget,
    },
    GenerateReport,
    NavigateNextStep,
    NavigateComplete,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PracticeFlowStep {
    pub template_id: PracticeStepTemplateId,
    pub word_advance: WordAdvance,
    pub on_end: Vec<PracticeEndAction>,
    pub shuffle_on_enter: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PracticeFlowNode {
    pub id: &'static str,
    pub label: &'static str,
    pub source: PracticeWordsSource,
    pub steps: Vec<PracticeFlowStep>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PracticeFlowConfig {
    pub id: &'static str,
    pub version: u32,
    pub mode: WordPracticeMode,
    pub label: &'static str,
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
        sub_steps: vec![PracticeStepTemplateId::Spell],
    }
}

fn default_on_end() -> Vec<PracticeEndAction> {
    vec![PracticeEndAction::WrongWordClear {
        template_id: PracticeStepTemplateId::FollowWrite,
        word_advance: word_loop_with_spell(),
    }]
}

/// 7 个内置流程（与 Web 的 BUILTIN_FLOWS 一致）。
pub fn builtin_flow(id: &str) -> Option<PracticeFlowConfig> {
    let config = match id {
        "system" => PracticeFlowConfig {
            id: "system",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::System,
            label: "smart_learning",
            nodes: vec![
                PracticeFlowNode {
                    id: "new",
                    label: "new_words",
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
                    id: "review",
                    label: "review",
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
            id: "free",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::Free,
            label: "free_practice",
            nodes: vec![PracticeFlowNode {
                id: "practice",
                label: "free_practice",
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
            id: "review",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::Review,
            label: "review",
            nodes: vec![PracticeFlowNode {
                id: "review",
                label: "review",
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
            id: "identifyOnly",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::IdentifyOnly,
            label: "identify",
            nodes: vec![
                PracticeFlowNode {
                    id: "new",
                    label: "new_words",
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![step(
                        PracticeStepTemplateId::Identify,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    )],
                },
                PracticeFlowNode {
                    id: "review",
                    label: "review",
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
            id: "dictationOnly",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::DictationOnly,
            label: "dictation",
            nodes: vec![
                PracticeFlowNode {
                    id: "new",
                    label: "new_words",
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![step(
                        PracticeStepTemplateId::Dictation,
                        WordAdvance::Increment,
                        default_on_end(),
                        true,
                    )],
                },
                PracticeFlowNode {
                    id: "review",
                    label: "review",
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
            id: "listenOnly",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::ListenOnly,
            label: "listen",
            nodes: vec![
                PracticeFlowNode {
                    id: "new",
                    label: "new_words",
                    source: PracticeWordsSource::TaskNew,
                    steps: vec![step(
                        PracticeStepTemplateId::Listen,
                        WordAdvance::Increment,
                        default_on_end(),
                        false,
                    )],
                },
                PracticeFlowNode {
                    id: "review",
                    label: "review",
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
            id: "shuffle",
            version: CURRENT_FLOW_VERSION,
            mode: WordPracticeMode::Shuffle,
            label: "random_review",
            nodes: vec![PracticeFlowNode {
                id: "practice",
                label: "random_review",
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
        .unwrap_or_else(|| &config.nodes[0].steps[0])
}

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
                word_advance: word_advance.clone(),
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
            practice_type: sub_step.practice_type(),
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
        return *sub_step;
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

    let mut status = tr(config.label);
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
            tr(node.label)
        } else {
            format!("{} · {}", tr(node.label), step_label)
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
                name: tr(node.label),
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
