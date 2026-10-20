// 用上游 ts-fsrs 生成 FSRS 黄金样本，供 Rust 端的 tests/fsrs_parity.rs 比对。
//
// 用法（在上游 TypeWords 克隆已 `pnpm install` 的前提下）：
//   node tools/fsrs_golden.mjs > tests/fixtures/fsrs_golden.json
//
// 脚本刻意不改动上游仓库：ts-fsrs 从上游的 node_modules 直接按绝对路径引入。

import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import fs from 'node:fs'

const here = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(here, '..')

// 上游克隆目录：可用环境变量覆盖
const upstreamRoot = process.env.TYPEWORDS_UPSTREAM ?? 'D:/Projects/TypeWords'
const tsFsrsEntry = path.join(upstreamRoot, 'node_modules', 'ts-fsrs', 'dist', 'index.mjs')

if (!fs.existsSync(tsFsrsEntry)) {
  console.error(
    `找不到 ts-fsrs：${tsFsrsEntry}\n` +
      `请先在 ${upstreamRoot} 执行 pnpm install，或用 TYPEWORDS_UPSTREAM 指定上游目录。`,
  )
  process.exit(1)
}

const { createEmptyCard, FSRS } = await import(`file://${tsFsrsEntry.replace(/\\/g, '/')}`)

// 固定参数：与上游 setting.ts 的默认值一致
const parameters = {
  request_retention: 0.9,
  maximum_interval: 36500,
  w: [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
  ],
  enable_fuzz: false,
  enable_short_term: true,
  learning_steps: ['1m', '10m'],
  relearning_steps: ['10m'],
}

const fuzzParameters = { ...parameters, enable_fuzz: true }

// 固定的复习时刻
const base = Date.UTC(2026, 0, 1, 8, 0, 0)
const day = 24 * 60 * 60 * 1000

/** 把卡片压成可比较的字段。 */
function snapshot(card) {
  return {
    due: card.due.toISOString(),
    stability: card.stability,
    difficulty: card.difficulty,
    elapsed_days: card.elapsed_days,
    scheduled_days: card.scheduled_days,
    learning_steps: card.learning_steps,
    reps: card.reps,
    lapses: card.lapses,
    state: card.state,
    last_review: card.last_review ? card.last_review.toISOString() : null,
  }
}

/**
 * 跑一串固定操作，返回每一步的结果。
 * steps: [{ at: 距 base 的毫秒, grade: 1..4 }]
 */
function run(fsrs, steps) {
  let card = createEmptyCard(new Date(base))
  const results = []
  for (const step of steps) {
    const now = new Date(base + step.at)
    card = fsrs.next(card, now, step.grade).card
    results.push(snapshot(card))
  }
  return results
}

const scenarios = [
  {
    name: 'new_card_sequence',
    parameters,
    // 新卡：Good → Again → Good → Good → Easy
    steps: [
      { at: 0, grade: 3 },
      { at: 0, grade: 1 },
      { at: 0, grade: 3 },
      { at: 0, grade: 3 },
      { at: 0, grade: 4 },
    ],
  },
  {
    name: 'graduated_to_review',
    parameters,
    // 先走出学习态，再按天复习
    steps: [
      { at: 0, grade: 3 },
      { at: 0, grade: 3 },
      { at: 0, grade: 3 },
      { at: day, grade: 3 },
      { at: 3 * day, grade: 1 },
      { at: 4 * day, grade: 3 },
    ],
  },
  {
    name: 'same_day_review',
    parameters,
    // 同日再复习会走短期记忆公式
    steps: [
      { at: 0, grade: 4 },
      { at: 60 * 60 * 1000, grade: 3 },
      { at: 2 * 60 * 60 * 1000, grade: 3 },
    ],
  },
  {
    name: 'with_fuzz',
    parameters: fuzzParameters,
    steps: [
      { at: 0, grade: 4 },
      { at: 0, grade: 3 },
      { at: day, grade: 3 },
      { at: 5 * day, grade: 3 },
    ],
  },
  {
    name: 'long_term_no_short_term',
    parameters: { ...parameters, enable_short_term: false },
    steps: [
      { at: 0, grade: 3 },
      { at: day, grade: 3 },
      { at: 5 * day, grade: 1 },
    ],
  },
]

const output = {
  note: '由 tools/fsrs_golden.mjs 用上游 ts-fsrs 生成；tests/fsrs_parity.rs 逐字段比对',
  base: new Date(base).toISOString(),
  scenarios: scenarios.map((scenario) => {
    const fsrs = new FSRS(scenario.parameters)
    return {
      name: scenario.name,
      parameters: scenario.parameters,
      inputs: scenario.steps.map((step) => ({ at: step.at, grade: step.grade })),
      results: run(fsrs, scenario.steps),
    }
  }),
}

process.stdout.write(JSON.stringify(output, null, 2) + '\n')
