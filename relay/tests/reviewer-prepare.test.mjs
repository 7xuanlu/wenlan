// SPDX-License-Identifier: Apache-2.0
// Unit tests for relay/reviewer/prepare.mjs. Temp files only, no live HTTP.

import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, lstatSync, symlinkSync, mkdirSync, statSync, existsSync, openSync, closeSync, writeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  parseArgs,
  renderHelp,
  prepare,
  validateMapping,
  validateTemplate,
  transformSubmission,
  containsExactToken,
  defaultTemplatePath,
  writeJsonAtZero,
} from '../reviewer/prepare.mjs';

const DAEMON = 'http://127.0.0.1:7879';
const KNOWLEDGE = '/tmp/knowledge-test-path';
const TEMPLATE = defaultTemplatePath();

function goodMapping() {
  return {
    'mem_atlas-auth': 'synMemAuth1',
    'mem_atlas-unavailable': 'synMemUnavail2',
    'mem_private-sentinel': 'synMemSent3',
    'page_atlas-auth': 'synPageAuth4',
    'page_atlas-unavailable': 'synPageUnavail5',
    'page_private-sentinel': 'synPageSent6',
  };
}

function consentArgs(output) {
  return { daemonUrl: DAEMON, knowledgePath: KNOWLEDGE, output, confirmSyntheticLibrary: true };
}

function seedOk(mapping = goodMapping()) {
  let calls = 0;
  const fn = async () => {
    calls += 1;
    return { mapping, warnings: [], pageReviewNotModified: [] };
  };
  fn.calls = () => calls;
  return fn;
}

let dir;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'reviewer-prepare-'));
});

describe('help', () => {
  it('rejects blank text and unsupported tools before seeding or reserving output', async () => {
    for (const change of [{ description: '  ' }, { tools_triggered: 'capture' }]) {
      const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
      Object.assign(template.test_cases[0], change);
      const path = join(dir, `invalid-${Object.keys(change)[0]}.json`);
      writeFileSync(path, JSON.stringify(template));
      const out = `${path}.receipt`;
      const seed = seedOk();
      await assert.rejects(prepare(consentArgs(out), { seedFn: seed, templatePath: path }),
        /nonempty|query-only/);
      assert.equal(seed.calls(), 0);
      assert.equal(existsSync(out), false);
    }
  });
  it('parses --help and renders usage with no writes', () => {
    assert.deepEqual(parseArgs(['--help']), { help: true });
    const help = renderHelp();
    assert.match(help, /--daemon-url/);
    assert.match(help, /--confirm-synthetic-library/);
  });
  it('rejects --help with other args', () => {
    assert.throws(() => parseArgs(['--help', '--daemon-url', DAEMON]), /--help/);
  });
});

describe('strict options', () => {
  const base = ['--daemon-url', DAEMON, '--knowledge-path', KNOWLEDGE, '--output', '/tmp/x.json'];
  it('rejects unknown argument', () => {
    assert.throws(() => parseArgs([...base, '--confirm-synthetic-library', '--bogus']), /unknown/);
  });
  it('rejects duplicate argument', () => {
    assert.throws(
      () => parseArgs([...base, '--confirm-synthetic-library', '--daemon-url', DAEMON]),
      /duplicate/,
    );
  });
  it('rejects missing value', () => {
    assert.throws(() => parseArgs(['--daemon-url']), /missing value/);
  });
  it('rejects missing confirmation', () => {
    assert.throws(() => parseArgs(base), /--confirm-synthetic-library/);
  });
  it('rejects missing required args', () => {
    assert.throws(() => parseArgs(['--confirm-synthetic-library']), /missing required/);
  });
  it('rejects value on confirm flag', () => {
    assert.throws(
      () => parseArgs([...base, '--confirm-synthetic-library=x']),
      /takes no value/,
    );
  });
  it('preserves confirmSyntheticLibrary:true', () => {
    const parsed = parseArgs([
      `--daemon-url=${DAEMON}`,
      `--knowledge-path=${KNOWLEDGE}`,
      '--output=/tmp/x.json',
      '--confirm-synthetic-library',
    ]);
    assert.equal(parsed.daemonUrl, DAEMON);
    assert.equal(parsed.confirmSyntheticLibrary, true);
  });
});

describe('URL, path, and consent validation (no seed, no write)', () => {
  it('denies default port 7878', async () => {
    const out = join(dir, 'o.json');
    const seed = seedOk();
    await assert.rejects(
      prepare(
        { daemonUrl: 'http://127.0.0.1:7878', knowledgePath: KNOWLEDGE, output: out, confirmSyntheticLibrary: true },
        { seedFn: seed, templatePath: TEMPLATE },
      ),
    );
    assert.equal(seed.calls(), 0);
    assert.equal(existsSync(out), false);
  });
  it('denies public host', async () => {
    const seed = seedOk();
    await assert.rejects(
      prepare(
        { daemonUrl: 'http://example.com:7879', knowledgePath: KNOWLEDGE, output: join(dir, 'o.json'), confirmSyntheticLibrary: true },
        { seedFn: seed, templatePath: TEMPLATE },
      ),
    );
    assert.equal(seed.calls(), 0);
  });
  it('rejects relative knowledge path', async () => {
    const seed = seedOk();
    await assert.rejects(
      prepare(
        { daemonUrl: DAEMON, knowledgePath: 'relative/path', output: join(dir, 'o.json'), confirmSyntheticLibrary: true },
        { seedFn: seed, templatePath: TEMPLATE },
      ),
    );
    assert.equal(seed.calls(), 0);
  });
  it('rejects relative output path', async () => {
    const seed = seedOk();
    await assert.rejects(
      prepare({ daemonUrl: DAEMON, knowledgePath: KNOWLEDGE, output: 'relative.json', confirmSyntheticLibrary: true }, {
        seedFn: seed,
        templatePath: TEMPLATE,
      }),
    );
    assert.equal(seed.calls(), 0);
  });
  it('direct prepare without consent writes nothing and never seeds', async () => {
    const out = join(dir, 'noconsent.json');
    const seed = seedOk();
    await assert.rejects(
      prepare({ daemonUrl: DAEMON, knowledgePath: KNOWLEDGE, output: out }, {
        seedFn: seed,
        templatePath: TEMPLATE,
      }),
      /confirm-synthetic-library/,
    );
    assert.equal(seed.calls(), 0);
    assert.equal(existsSync(out), false);
  });
});

describe('template validation (no seed, no write)', () => {
  function writeTemplate(name, mutate) {
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    mutate(template);
    const path = join(dir, name);
    writeFileSync(path, JSON.stringify(template));
    return path;
  }
  it('rejects wrong positive count', async () => {
    const bad = writeTemplate('bad-count.json', (t) => {
      t.test_cases = t.test_cases.slice(0, 4);
    });
    const out = join(dir, 'o.json');
    const seed = seedOk();
    await assert.rejects(
      prepare(consentArgs(out), { seedFn: seed, templatePath: bad }),
      /five positive/,
    );
    assert.equal(seed.calls(), 0);
    assert.equal(existsSync(out), false);
  });
  it('rejects empty positive description and non-null negative tool name', () => {
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    const emptied = structuredClone(template);
    emptied.test_cases[0].description = '';
    assert.throws(() => validateTemplate(emptied), /test_cases\[0\]\.description/);
    const nullTool = structuredClone(template);
    nullTool.test_cases[1].tools_triggered = null;
    assert.throws(() => validateTemplate(nullTool), /tools_triggered/);
    const negTool = structuredClone(template);
    negTool.negative_test_cases[0].tools_triggered = 'recall';
    assert.throws(() => validateTemplate(negTool), /negative_test_cases\[0\]/);
    const negPrompt = structuredClone(template);
    negPrompt.negative_test_cases[2].user_prompt = '';
    assert.throws(() => validateTemplate(negPrompt), /negative_test_cases\[2\]\.user_prompt/);
  });
  it('rejects empty positive user_prompt and expected_output', () => {
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    const badPrompt = structuredClone(template);
    badPrompt.test_cases[2].user_prompt = '';
    assert.throws(() => validateTemplate(badPrompt), /user_prompt/);
    const badExpected = structuredClone(template);
    badExpected.test_cases[3].expected_output = '';
    assert.throws(() => validateTemplate(badExpected), /expected_output/);
  });
  it('no seed when template unreadable', async () => {
    const seed = seedOk();
    await assert.rejects(
      prepare(consentArgs(join(dir, 'o.json')), {
        seedFn: seed,
        templatePath: join(dir, 'nope.json'),
      }),
      /template/,
    );
    assert.equal(seed.calls(), 0);
  });
});

describe('output reservation (no seed on conflict)', () => {
  it('refuses existing file and preserves content', async () => {
    const out = join(dir, 'exists.json');
    writeFileSync(out, '{"keep":true}\n');
    const seed = seedOk();
    await assert.rejects(
      prepare(consentArgs(out), { seedFn: seed, templatePath: TEMPLATE }),
      /existing output/,
    );
    assert.equal(seed.calls(), 0);
    assert.equal(readFileSync(out, 'utf8'), '{"keep":true}\n');
  });
  it('refuses symlink without following it', async () => {
    const target = join(dir, 'target.json');
    writeFileSync(target, 'secret');
    const out = join(dir, 'link.json');
    symlinkSync(target, out);
    assert.ok(lstatSync(out).isSymbolicLink());
    const seed = seedOk();
    await assert.rejects(
      prepare(consentArgs(out), { seedFn: seed, templatePath: TEMPLATE }),
      /existing output/,
    );
    assert.equal(seed.calls(), 0);
    assert.equal(readFileSync(target, 'utf8'), 'secret');
  });
  it('refuses directory', async () => {
    const out = join(dir, 'subdir');
    mkdirSync(out);
    const seed = seedOk();
    await assert.rejects(prepare(consentArgs(out), { seedFn: seed, templatePath: TEMPLATE }));
    assert.equal(seed.calls(), 0);
  });
});

describe('prepared receipt', () => {
  it('completes partial byte writes and refuses a zero-progress write', () => {
    const output = join(dir, 'partial-write.json');
    const fd = openSync(output, 'wx', 0o600);
    try {
      const payload = { status: 'prepared', note: '\u6587\u703e' };
      let calls = 0;
      writeJsonAtZero(fd, payload, (handle, bytes, offset, length, position) => {
        calls += 1;
        return writeSync(handle, bytes, offset, Math.min(3, length), position);
      });
      assert(calls > 1);
      assert.deepEqual(JSON.parse(readFileSync(output, 'utf8')), payload);
      assert.throws(() => writeJsonAtZero(fd, payload, () => 0), /valid progress/);
    } finally {
      closeSync(fd);
    }
  });
  it('writes top-level test_cases via same fd, mode 0600, verification unchecked', async () => {
    const out = join(dir, 'receipt.json');
    const mapping = goodMapping();
    const seed = seedOk(mapping);
    const receipt = await prepare(consentArgs(out), { seedFn: seed, templatePath: TEMPLATE });
    assert.equal(seed.calls(), 1);
    assert.equal(receipt.status, 'prepared');
    assert.deepEqual(receipt.fixture.mapping, mapping);
    assert.ok(!('generatedSubmission' in receipt));
    assert.ok(!('mapping' in receipt));
    const onDisk = JSON.parse(readFileSync(out, 'utf8'));
    assert.equal(onDisk.status, 'prepared');
    assert.equal(onDisk.test_cases.length, 5);
    assert.equal(onDisk.negative_test_cases.length, 3);
    const serialized = JSON.stringify({ t: onDisk.test_cases, n: onDisk.negative_test_cases });
    assert.ok(serialized.includes(mapping['mem_atlas-auth']));
    assert.ok(serialized.includes(mapping['page_atlas-auth']));
    assert.ok(serialized.includes(mapping['page_atlas-unavailable']));
    for (const token of ['mem_atlas-auth', 'page_atlas-auth', 'page_atlas-unavailable']) {
      assert.ok(!containsExactToken(serialized, token), `residual ${token}`);
    }
    // Private-sentinel IDs stay out of public prompts (template lacks them).
    assert.ok(!serialized.includes(mapping['mem_private-sentinel']));
    assert.ok(!serialized.includes(mapping['page_private-sentinel']));
    // Fixture receipt itself may carry all synthetic IDs.
    assert.ok(JSON.stringify(onDisk.fixture.mapping).includes(mapping['mem_private-sentinel']));
    assert.deepEqual(onDisk.verification, {
      installation: 'unchecked',
      chatgpt: 'unchecked',
      oauth: 'unchecked',
      relay: 'unchecked',
      reviewAcceptance: 'unchecked',
    });
    const mode = statSync(out).mode & 0o777;
    assert.equal(mode, 0o600);
  });
  it('keeps tool names and leaves negative cases unchanged', async () => {
    const out = join(dir, 'receipt2.json');
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    const receipt = await prepare(consentArgs(out), { seedFn: seedOk(), templatePath: TEMPLATE });
    assert.deepEqual(
      receipt.test_cases.map((c) => c.tools_triggered),
      template.test_cases.map((c) => c.tools_triggered),
    );
    assert.deepEqual(receipt.negative_test_cases, template.negative_test_cases);
  });
});

describe('exact-token one-pass replacement', () => {
  it('requires every public fixture binding before any mutation', async () => {
    for (const token of ['mem_atlas-auth', 'page_atlas-auth', 'page_atlas-unavailable']) {
      const text = readFileSync(TEMPLATE, 'utf8').replaceAll(token, 'missing-fixture-reference');
      const templatePath = join(dir, `${token}.json`);
      writeFileSync(templatePath, text);
      const output = `${templatePath}.receipt`;
      const seed = seedOk();
      await assert.rejects(prepare(consentArgs(output), { seedFn: seed, templatePath }), /fixture binding/);
      assert.equal(seed.calls(), 0);
      assert.equal(existsSync(output), false);
    }
  });
  it('does not mistake a substring for a private fixture ID', () => {
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    const result = transformSubmission(template, { ...goodMapping(), 'mem_private-sentinel': 'ead' });
    assert.equal(result.testCases.length, 5);
  });
  it('does not replace substrings inside longer identifiers', () => {
    assert.equal(containsExactToken('id mem_atlas-authX here', 'mem_atlas-auth'), false);
    assert.equal(containsExactToken('id Xmem_atlas-auth here', 'mem_atlas-auth'), false);
    assert.equal(containsExactToken('see mem_atlas-auth.', 'mem_atlas-auth'), true);
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    const doctored = structuredClone(template);
    doctored.test_cases[0].user_prompt += ' mem_atlas-authX';
    const { testCases } = transformSubmission(doctored, goodMapping());
    assert.ok(testCases[0].user_prompt.endsWith(' mem_atlas-authX'));
  });
  it('is one-pass: substituted values are never re-scanned', () => {
    const template = JSON.parse(readFileSync(TEMPLATE, 'utf8'));
    const mapping = { ...goodMapping(), 'mem_atlas-auth': 'Xpage_atlas-authY' };
    const { testCases } = transformSubmission(template, mapping);
    const serialized = JSON.stringify(testCases);
    // The substituted value survives literally: the embedded page token text
    // inside it was not re-scanned (a cascading replace would yield
    // 'XsynPageAuth4Y' instead).
    assert.ok(serialized.includes('Xpage_atlas-authY'));
    assert.ok(!serialized.includes(`X${mapping['page_atlas-auth']}Y`));
  });
});

describe('mapping validation', () => {
  it('denies malformed, duplicate, missing, and historical-token IDs', () => {
    assert.throws(() => validateMapping({ ...goodMapping(), 'mem_atlas-auth': 'bad id!' }), /malformed/);
    assert.throws(
      () =>
        validateMapping({
          'mem_atlas-auth': 'dup',
          'mem_atlas-unavailable': 'dup',
          'mem_private-sentinel': 'a3',
          'page_atlas-auth': 'a4',
          'page_atlas-unavailable': 'a5',
          'page_private-sentinel': 'a6',
        }),
      /duplicate/,
    );
    const incomplete = goodMapping();
    delete incomplete['page_atlas-auth'];
    assert.throws(() => validateMapping(incomplete), /malformed/);
    assert.throws(
      () => validateMapping({ ...goodMapping(), 'mem_atlas-auth': 'page_atlas-auth' }),
      /historical token/,
    );
  });
  it('writes failed receipt with partial warning when seed fails', async () => {
    const out = join(dir, 'failed.json');
    const seed = async () => {
      throw new Error('boom seed');
    };
    await assert.rejects(
      prepare(consentArgs(out), { seedFn: seed, templatePath: TEMPLATE }),
      /partial data may remain/,
    );
    const onDisk = JSON.parse(readFileSync(out, 'utf8'));
    assert.equal(onDisk.status, 'failed');
    assert.equal(onDisk.stage, 'seed');
    assert.equal(onDisk.partialDataMayRemain, true);
    assert.match(onDisk.warning, /partial/i);
    assert.match(onDisk.warning, /No automatic rerun/i);
  });
  it('writes failed receipt when mapping validation fails after seed', async () => {
    const out = join(dir, 'failed2.json');
    const bad = { ...goodMapping(), 'mem_atlas-auth': 'bad id!' };
    await assert.rejects(
      prepare(consentArgs(out), { seedFn: seedOk(bad), templatePath: TEMPLATE }),
      /malformed generated ID/,
    );
    const onDisk = JSON.parse(readFileSync(out, 'utf8'));
    assert.equal(onDisk.status, 'failed');
    assert.equal(onDisk.partialDataMayRemain, true);
  });
});
