// SPDX-License-Identifier: Apache-2.0
// Offline only. Enrollment and publishing the secret require separate approval.
import { mkdir, realpath, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join } from 'node:path';
import { parseArgs } from 'node:util';
import { readPrivateJson } from './private-json.mjs';
import { prepareNewDeviceSampleAccount } from '../src/sample-account.ts';

try {
  const { values } = parseArgs({ options: {
    enrollment: { type: 'string' }, space: { type: 'string' }, resource: { type: 'string' },
    output: { type: 'string' }, username: { type: 'string', default: 'reviewer' },
    'synthetic-library': { type: 'boolean', default: false },
  }, strict: true, allowPositionals: false });
  if (!values['synthetic-library'] || !values.enrollment || !values.output || !values.space || !values.resource
    || !isAbsolute(values.enrollment) || !isAbsolute(values.output) || !process.getuid) {
    throw new Error('arguments');
  }
  const enrollment = await readPrivateJson(values.enrollment);
  const prepared = await prepareNewDeviceSampleAccount(enrollment, {
    username: values.username, resource: values.resource, space: values.space,
  });
  if (!prepared) throw new Error('invalid enrollment');
  const parent = await realpath(dirname(values.output));
  const parentInfo = await stat(parent);
  if (!parentInfo.isDirectory() || parentInfo.uid !== process.getuid() || (parentInfo.mode & 0o022) !== 0) {
    throw new Error('trusted output parent required');
  }
  const output = join(parent, basename(values.output));
  await mkdir(output, { mode: 0o700 });
  await writeFile(join(output, 'sample-account.json'), JSON.stringify(prepared.account) + '\n', { flag: 'wx', mode: 0o600 });
  await writeFile(join(output, 'reviewer-password.txt'), prepared.password + '\n', { flag: 'wx', mode: 0o600 });
  process.stdout.write('Prepared private account files. No network request or deployment performed.\n');
} catch {
  // Never print supplied values, raw JSON or errors which may contain secrets.
  process.stderr.write('Preparation failed. Require a private fresh-enrollment JSON file, explicit synthetic-library flag, valid Space/HTTPS MCP resource, and a new output directory under an owned non-writable-by-others parent. Existing or partial output is never overwritten.\n');
  process.exitCode = 1;
}
