/**
 * Publish gate: require a green Rust CI workflow run on the tag commit.
 */

import { spawnSync } from 'node:child_process';

const REPO = process.env.GITHUB_REPOSITORY || 'nyar-vm/nyar-vm.rs';
const TAG_SHA = String(process.env.TAG_SHA || process.argv.find((a) => a.startsWith('--sha='))?.slice(6) || '')
    .trim()
    .toLowerCase();
const WORKFLOW = process.env.CI_WORKFLOW_FILE || 'check-rust.yml';
const TIMEOUT_MS = Number(process.env.CI_WAIT_TIMEOUT_MS || 45 * 60 * 1000);
const INTERVAL_MS = Number(process.env.CI_WAIT_INTERVAL_MS || 20 * 1000);
const REQUIRED_JOBS = ['build'];

function fail(msg) {
    console.error(`require-ci-success: ${msg}`);
    process.exit(1);
}

function gh(args) {
    const r = spawnSync('gh', args, {
        encoding: 'utf8',
        shell: false,
        env: { ...process.env, GH_TOKEN: process.env.GH_TOKEN || process.env.GITHUB_TOKEN || '' },
    });
    if (r.status !== 0) {
        fail(r.stderr?.trim() || r.stdout?.trim() || `gh ${args.join(' ')} exit ${r.status}`);
    }
    return String(r.stdout ?? '').trim();
}

function listRuns() {
    const raw = gh([
        'run',
        'list',
        '--repo',
        REPO,
        '--commit',
        TAG_SHA,
        '--workflow',
        WORKFLOW,
        '--limit',
        '20',
        '--json',
        'databaseId,status,conclusion,createdAt,displayTitle,event',
    ]);
    try {
        return JSON.parse(raw);
    } catch {
        fail(`invalid gh json: ${raw.slice(0, 200)}`);
    }
}

function requiredJobsSucceeded(runId) {
    const raw = gh(['api', `repos/${REPO}/actions/runs/${runId}/jobs?per_page=100`]);
    let jobs;
    try {
        jobs = JSON.parse(raw).jobs ?? [];
    } catch {
        return false;
    }
    const byName = new Map(jobs.map((job) => [job.name, job]));
    for (const name of REQUIRED_JOBS) {
        const job = byName.get(name);
        if (!job || job.conclusion !== 'success') {
            return false;
        }
    }
    return true;
}

function sleep(ms) {
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

if (!TAG_SHA) fail('need TAG_SHA or --sha=');

console.log(`require-ci-success: repo=${REPO} sha=${TAG_SHA} workflow=${WORKFLOW}`);

const deadline = Date.now() + TIMEOUT_MS;
while (Date.now() < deadline) {
    const runs = listRuns();
    const completed = runs.filter((run) => run.status === 'completed');
    const success = completed.find((run) => run.conclusion === 'success' && requiredJobsSucceeded(run.databaseId));
    if (success) {
        console.log(`require-ci-success: green run ${success.databaseId} (${success.displayTitle})`);
        process.exit(0);
    }
    const inFlight = runs.some((run) => run.status === 'in_progress' || run.status === 'queued');
    if (!inFlight && completed.length > 0) {
        fail(`no successful ${WORKFLOW} run on ${TAG_SHA}`);
    }
    console.log(`require-ci-success: waiting for ${WORKFLOW} on ${TAG_SHA}…`);
    sleep(INTERVAL_MS);
}

fail(`timeout waiting for green ${WORKFLOW} on ${TAG_SHA}`);
