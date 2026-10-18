#!/usr/bin/env node
/**
 * 把上游 TypeWords 仓库的词库资源同步到本仓库的 assets/ 目录。
 *
 * 用法：
 *   node scripts/sync-assets.mjs                      # 默认从 ../TypeWords/public 同步
 *   node scripts/sync-assets.mjs --source <path>      # 指定 TypeWords 仓库路径
 *   node scripts/sync-assets.mjs --out <dir>          # 指定输出目录（默认 assets）
 *
 * 产物（已提交进仓库，运行时不需要 Node）：
 *   assets/list/word.json
 *   assets/dicts/en/word/*.json
 */
import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
const argValue = (name) => { const index = args.indexOf(name); return index > -1 ? args[index + 1] : undefined }

const source = resolve(root, argValue('--source') ?? process.env.TYPEWORDS_SOURCE ?? '../TypeWords')
const target = resolve(root, argValue('--out') ?? 'assets')

if (!existsSync(resolve(source, 'public', 'list', 'word.json'))) {
  console.error('[sync-assets] 找不到上游词库目录，请用 --source 指定 TypeWords 仓库路径:', source)
  process.exit(1)
}

rmSync(target, { recursive: true, force: true })

const copy = (relative) => {
  const from = resolve(source, 'public', relative)
  const to = resolve(target, relative)
  if (!existsSync(from)) {
    console.warn('[sync-assets] 源文件不存在，跳过:', from)
    return
  }
  mkdirSync(dirname(to), { recursive: true })
  cpSync(from, to, { recursive: true })
  console.log('[sync-assets] 复制', from, '→', to)
}

copy('list/word.json')
copy('dicts/en/word')
console.log('[sync-assets] 完成:', target)
