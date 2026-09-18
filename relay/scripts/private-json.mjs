// SPDX-License-Identifier: Apache-2.0
import { constants } from 'node:fs';
import { open } from 'node:fs/promises';
import { isAbsolute } from 'node:path';

const MAX_PRIVATE_JSON_BYTES = 4096;

export async function readPrivateJson(path) {
  if (typeof path !== 'string' || !isAbsolute(path)) {
    throw new Error('absolute path required');
  }
  if (typeof process.getuid !== 'function' || typeof constants.O_NOFOLLOW !== 'number'
    || typeof constants.O_NONBLOCK !== 'number') {
    throw new Error('POSIX private input required');
  }

  const handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const info = await handle.stat();
    if (!info.isFile() || info.uid !== process.getuid() || (info.mode & 0o077) !== 0) {
      throw new Error('private input required');
    }
    if (info.size > MAX_PRIVATE_JSON_BYTES) throw new Error('input size');

    const buffer = Buffer.alloc(MAX_PRIVATE_JSON_BYTES + 1);
    try {
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, 0);
      if (bytesRead > MAX_PRIVATE_JSON_BYTES) throw new Error('input size');
      const text = new TextDecoder('utf-8', { fatal: true }).decode(buffer.subarray(0, bytesRead));
      return JSON.parse(text);
    } finally {
      buffer.fill(0);
    }
  } finally {
    await handle.close();
  }
}
