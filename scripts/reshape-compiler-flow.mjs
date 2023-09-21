#!/usr/bin/env node

/**
 * 编译流一次性目录重塑器。
 *
 * 这个脚本只处理已经完成语义迁移后的机械重命名，不生产任何语义合同，
 * 也不把旧 FragmentSubmission 包装成新载荷。默认只生成计划；--apply 会
 * 在所有旧成功入口清除后执行明确的文件移动和引用重写。
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const apply = process.argv.includes("--apply");
const planOnly = process.argv.includes("--plan") || !apply;

const moves = [
  [
    "projects/compilers/nyar-language/src/valkyrie/compile_pipeline/producer.rs",
    "projects/compilers/nyar-language/src/valkyrie/compile_pipeline/canonical.rs",
  ],
  [
    "projects/compilers/nyar-language/src/valkyrie/compile_pipeline/planner.rs",
    "projects/compilers/nyar-language/src/valkyrie/compile_pipeline/representation.rs",
  ],
  [
    "projects/compilers/nyar-language/src/valkyrie/assembly",
    "projects/compilers/nyar-language/src/valkyrie/compile_pipeline/artifact",
  ],
];

const rewrites = [
  ["mod producer;", "mod canonical;"],
  ["mod planner;", "mod representation;"],
  ["pub use producer::", "pub use canonical::"],
  ["pub use planner::", "pub use representation::"],
  ["pub mod assembly;", ""],
  ["crate::valkyrie::assembly::", "crate::valkyrie::compile_pipeline::artifact::"],
  ["valkyrie::assembly::", "valkyrie::compile_pipeline::artifact::"],
];

const forbiddenProductionSymbols = [
  "FragmentSubmission",
  "from_fragment_submission",
  "mir_function_to_executable",
  "resolve_static_callee_operation",
  "find_by_symbol",
  "operation_for_exact_symbol",
];

function absolute(relative) {
  const resolved = path.resolve(root, relative);
  const relativeToRoot = path.relative(root, resolved);
  if (relativeToRoot.startsWith("..") || path.isAbsolute(relativeToRoot)) {
    throw new Error(`路径越过仓库边界: ${relative}`);
  }
  return resolved;
}

function exists(relative) {
  return fs.existsSync(absolute(relative));
}

function productionRustFiles() {
  const files = [];
  const roots = ["projects/compilers/nyar-language/src", "projects/compilers/nyar-emitter/src"];
  const visit = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) visit(full);
      else if (entry.isFile() && entry.name.endsWith(".rs")) files.push(full);
    }
  };
  for (const relative of roots) visit(absolute(relative));
  return files;
}

function legacyOccurrences() {
  const occurrences = [];
  for (const file of productionRustFiles()) {
    const text = fs.readFileSync(file, "utf8");
    for (const symbol of forbiddenProductionSymbols) {
      if (!text.includes(symbol)) continue;
      occurrences.push(`${path.relative(root, file)}: ${symbol}`);
    }
  }
  return occurrences;
}

function validatePlan() {
  const errors = [];
  for (const [source, destination] of moves) {
    if (!exists(source)) errors.push(`缺少迁移源: ${source}`);
    if (exists(destination)) errors.push(`迁移目标已存在: ${destination}`);
  }
  const legacy = legacyOccurrences();
  if (legacy.length) {
    errors.push("生产语义旁路尚未清除，拒绝目录重塑:");
    errors.push(...legacy);
  }
  return errors;
}

function printPlan(errors) {
  console.log("编译流目录重塑计划:");
  for (const [source, destination] of moves) console.log(`  ${source} -> ${destination}`);
  console.log("引用重写:");
  for (const [from, to] of rewrites) console.log(`  ${JSON.stringify(from)} -> ${JSON.stringify(to)}`);
  if (errors.length) {
    console.error("\n当前不能执行 --apply:");
    for (const error of errors) console.error(`  ${error}`);
    console.error("先完成 Compiler -> CanonicalProgram -> RepresentationPlan -> BackendPrivatePlan 的语义切换。");
  }
}

function rewriteRustReferences() {
  for (const file of productionRustFiles()) {
    const original = fs.readFileSync(file, "utf8");
    let updated = original;
    for (const [from, to] of rewrites) updated = updated.split(from).join(to);
    if (updated !== original) fs.writeFileSync(file, updated);
  }
}

function move(source, destination) {
  const sourcePath = absolute(source);
  const destinationPath = absolute(destination);
  fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
  fs.renameSync(sourcePath, destinationPath);
}

const errors = validatePlan();
printPlan(errors);

if (planOnly) process.exit(0);
if (errors.length) process.exit(2);

rewriteRustReferences();
for (const [source, destination] of moves) move(source, destination);
console.log("\n目录重塑完成；现在必须运行 rustfmt、cargo check 和完整合同回归。");
