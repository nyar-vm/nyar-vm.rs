import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { applyMigration, planMigration } from "./reshape-compiler-flow.mjs";

const segments = ["projects", "compilers", "nyar-language", "src", "valkyrie", "compile_pipeline"];
const original = "mod producer;\nmod planner;\npub use producer::produce;\npub use planner::plan;\n";

function fixture(context) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "compiler-reshape-"));
  const directory = path.join(root, ...segments);
  fs.mkdirSync(directory, { recursive: true });
  for (const name of ["producer.rs", "planner.rs"]) fs.writeFileSync(path.join(directory, name), name);
  fs.writeFileSync(path.join(directory, "mod.rs"), original);
  context.after(() => {
    for (const name of ["producer.rs", "planner.rs", "canonical.rs", "representation.rs", "mod.rs"]) {
      const file = path.join(directory, name);
      if (fs.existsSync(file)) fs.unlinkSync(file);
    }
    for (let depth = segments.length; depth >= 0; depth--) fs.rmdirSync(path.join(root, ...segments.slice(0, depth)));
  });
  return { root, directory };
}

test("预览不修改文件，迁移保留内容且再次执行无改动", (context) => {
  const { root, directory } = fixture(context);
  const plan = planMigration(root);
  assert.equal(plan.pending.length, 2);
  assert.equal(fs.readFileSync(path.join(directory, "mod.rs"), "utf8"), original);
  applyMigration(root, plan);
  assert.equal(fs.readFileSync(path.join(directory, "canonical.rs"), "utf8"), "producer.rs");
  assert.equal(fs.readFileSync(path.join(directory, "representation.rs"), "utf8"), "planner.rs");
  const finished = planMigration(root);
  assert.equal(finished.pending.length, 0);
  applyMigration(root, finished);
  assert.deepEqual(planMigration(root), finished);
});

test("目标冲突时不改动源或模块", (context) => {
  const { root, directory } = fixture(context);
  fs.writeFileSync(path.join(directory, "canonical.rs"), "existing");
  assert.throws(() => planMigration(root), /恰好存在一个/);
  assert.equal(fs.readFileSync(path.join(directory, "producer.rs"), "utf8"), "producer.rs");
  assert.equal(fs.readFileSync(path.join(directory, "mod.rs"), "utf8"), original);
});

test("缺模块引用或重复引用不能通过预检", (context) => {
  const { root, directory } = fixture(context);
  for (const invalid of [original.replace("mod producer;", ""), `${original}mod producer;\n`]) {
    fs.writeFileSync(path.join(directory, "mod.rs"), invalid);
    assert.throws(() => planMigration(root), /恰好出现一次/);
    assert.equal(fs.existsSync(path.join(directory, "producer.rs")), true);
  }
});

test("预览后内容变化拒绝过期计划", (context) => {
  const { root, directory } = fixture(context);
  const plan = planMigration(root);
  fs.appendFileSync(path.join(directory, "mod.rs"), "\n");
  assert.throws(() => applyMigration(root, plan), /过期计划/);
  assert.equal(fs.existsSync(path.join(directory, "producer.rs")), true);
});

test("模块名改动不能先于实际文件迁移", (context) => {
  const { root, directory } = fixture(context);
  fs.writeFileSync(path.join(directory, "mod.rs"), original.replace("mod producer;", "mod canonical;"));
  assert.throws(() => planMigration(root), /状态不一致/);
});
