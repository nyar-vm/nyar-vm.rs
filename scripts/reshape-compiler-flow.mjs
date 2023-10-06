#!/usr/bin/env node

/**
 * 编译流模块的机械迁移器，不生成或转换语义合同。
 * 默认预览；--apply 要求干净 checkpoint，完成预检后才修改明确列出的文件。
 * 文件重命名不证明 BackendPrivatePlan 已接入，重复执行不产生新的改动。
 */
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const directory = "projects/compilers/nyar-language/src/valkyrie/compile_pipeline";
const moves = [["producer.rs", "canonical.rs"], ["planner.rs", "representation.rs"]];
const rewrites = [
  ["mod producer;", "mod canonical;"],
  ["mod planner;", "mod representation;"],
  ["pub use producer::", "pub use canonical::"],
  ["pub use planner::", "pub use representation::"],
];

/** 只允许仓内普通文件，拒绝越界路径及符号链接。 */
function checkedFile(root, relative) {
  const resolved = path.resolve(root, relative);
  const within = path.relative(root, resolved);
  if (within === ".." || within.startsWith(`..${path.sep}`) || path.isAbsolute(within)) {
    throw new Error(`路径越过仓库边界: ${relative}`);
  }
  let current = root;
  for (const segment of within.split(path.sep)) {
    current = path.join(current, segment);
    if (fs.existsSync(current) && fs.lstatSync(current).isSymbolicLink()) {
      throw new Error(`迁移路径包含符号链接: ${relative}`);
    }
  }
  if (fs.existsSync(resolved) && !fs.lstatSync(resolved).isFile()) {
    throw new Error(`迁移路径不是普通文件: ${relative}`);
  }
  return resolved;
}

/** 先验证全部移动与精确模块引用，任何冲突都不写文件。 */
export function planMigration(root) {
  const pending = [];
  const completed = [];
  for (const [source, destination] of moves) {
    const sourceRelative = `${directory}/${source}`;
    const destinationRelative = `${directory}/${destination}`;
    const hasSource = fs.existsSync(checkedFile(root, sourceRelative));
    const hasDestination = fs.existsSync(checkedFile(root, destinationRelative));
    if (hasSource === hasDestination) {
      throw new Error(`迁移要求源或目标恰好存在一个: ${sourceRelative} -> ${destinationRelative}`);
    }
    (hasSource ? pending : completed).push([sourceRelative, destinationRelative]);
  }
  const moduleRelative = `${directory}/mod.rs`;
  const original = fs.readFileSync(checkedFile(root, moduleRelative), "utf8");
  let updated = original;
  for (const [from, to] of rewrites) {
    const oldCount = original.split(from).length - 1;
    const newCount = original.split(to).length - 1;
    if (oldCount + newCount !== 1) throw new Error(`模块引用必须恰好出现一次: ${from} / ${to}`);
    const index = from.includes("producer") ? 0 : 1;
    const hasSource = fs.existsSync(checkedFile(root, `${directory}/${moves[index][0]}`));
    if (hasSource !== (oldCount === 1)) throw new Error(`模块引用与文件状态不一致: ${from}`);
    updated = updated.replace(from, to);
  }
  return { pending, completed, moduleRelative, original, updated };
}

/** 按预检计划执行；不扫描其他模块，不创建兼容转发文件。 */
export function applyMigration(root, plan) {
  const current = planMigration(root);
  if (JSON.stringify(current) !== JSON.stringify(plan)) throw new Error("预览后文件发生变化，拒绝执行过期计划");
  for (const [source, destination] of plan.pending) {
    fs.renameSync(checkedFile(root, source), checkedFile(root, destination));
  }
  if (plan.original !== plan.updated) fs.writeFileSync(checkedFile(root, plan.moduleRelative), plan.updated);
}

function main() {
  const args = process.argv.slice(2);
  if (args.some((argument) => !["--plan", "--apply"].includes(argument)) ||
      (args.includes("--plan") && args.includes("--apply"))) {
    throw new Error("用法: node scripts/reshape-compiler-flow.mjs [--plan | --apply]");
  }
  const root = fs.realpathSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."));
  const plan = planMigration(root);
  console.log("机械迁移计划（不代表语义链接入）:");
  for (const [source, destination] of plan.pending) console.log(`待迁移: ${source} -> ${destination}`);
  for (const [, destination] of plan.completed) console.log(`已迁移: ${destination}`);
  if (!args.includes("--apply")) return;
  const status = execFileSync("git", ["status", "--porcelain", "--untracked-files=all"], { cwd: root, encoding: "utf8" });
  if (status.trim()) throw new Error("工作树不干净；先审查并提交 checkpoint，拒绝混合执行迁移");
  applyMigration(root, plan);
  console.log(plan.pending.length ? "机械迁移完成；仍需编译与合同验证。" : "迁移已完成，无新增改动。");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 2;
  }
}
