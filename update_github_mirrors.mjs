import vm from 'node:vm';
import { writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

export const USERSCRIPT_BLOB_URL =
  'https://github.com/XIU2/UserScript/blob/master/GithubEnhanced-High-Speed-Download.user.js';
export const USERSCRIPT_RAW_URL = USERSCRIPT_BLOB_URL
  .replace('https://github.com/', 'https://raw.githubusercontent.com/')
  .replace('/blob/', '/');

export const DOWNLOAD_PROBE_ORIGIN =
  'https://github.com/XIU2/CloudflareSpeedTest/releases/download/v2.2.2/CloudflareST_windows_amd64.zip';
export const RAW_PROBE_ORIGIN =
  'https://raw.githubusercontent.com/XIU2/UserScript/master/GithubEnhanced-High-Speed-Download.user.js';
export const RAW_PROBE_EXPECTED_TEXT = 'Github Enhancement - High Speed Download';

const DEFAULT_TIMEOUT_MS = 10_000;
const MIN_DOWNLOAD_BYTES = 20_000;
const USER_AGENT =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36';
const IDENTIFIER_CHARS = /[A-Za-z0-9_$]/;
const HTML_ENTITIES = new Map([
  ['amp', '&'],
  ['lt', '<'],
  ['gt', '>'],
  ['quot', '"'],
  ['apos', "'"],
  ['nbsp', ' '],
]);

function isIdentifierChar(char) {
  return char ? IDENTIFIER_CHARS.test(char) : false;
}

function decodeHtmlEntities(value) {
  return value.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z]+);/g, (_, entity) => {
    if (entity.startsWith('#x') || entity.startsWith('#X')) {
      return String.fromCodePoint(Number.parseInt(entity.slice(2), 16));
    }

    if (entity.startsWith('#')) {
      return String.fromCodePoint(Number.parseInt(entity.slice(1), 10));
    }

    return HTML_ENTITIES.get(entity) ?? `&${entity};`;
  });
}

function normalizeText(value) {
  return decodeHtmlEntities(String(value ?? '')).trim();
}

function skipQuotedString(source, startIndex, quote) {
  let index = startIndex + 1;

  while (index < source.length) {
    const char = source[index];

    if (char === '\\') {
      index += 2;
      continue;
    }

    if (quote === '`' && char === '$' && source[index + 1] === '{') {
      index += 2;
      let depth = 1;
      while (index < source.length && depth > 0) {
        const inner = source[index];
        if (inner === '\\') {
          index += 2;
          continue;
        }
        if (inner === '{') {
          depth += 1;
        } else if (inner === '}') {
          depth -= 1;
        } else if (inner === "'" || inner === '"' || inner === '`') {
          index = skipQuotedString(source, index, inner);
          continue;
        }
        index += 1;
      }
      continue;
    }

    if (char === quote) {
      return index + 1;
    }

    index += 1;
  }

  throw new Error(`Unterminated string literal starting at ${startIndex}`);
}

function skipComment(source, startIndex) {
  if (source[startIndex + 1] === '/') {
    let index = startIndex + 2;
    while (index < source.length && source[index] !== '\n') {
      index += 1;
    }
    return index;
  }

  let index = startIndex + 2;
  while (index < source.length) {
    if (source[index] === '*' && source[index + 1] === '/') {
      return index + 2;
    }
    index += 1;
  }

  throw new Error(`Unterminated block comment starting at ${startIndex}`);
}

function findArrayStart(source, variableName) {
  let index = 0;

  while (index < source.length) {
    const char = source[index];
    const next = source[index + 1];

    if (char === '/' && (next === '/' || next === '*')) {
      index = skipComment(source, index);
      continue;
    }

    if (char === "'" || char === '"' || char === '`') {
      index = skipQuotedString(source, index, char);
      continue;
    }

    if (
      source.startsWith(variableName, index) &&
      !isIdentifierChar(source[index - 1]) &&
      !isIdentifierChar(source[index + variableName.length])
    ) {
      let cursor = index + variableName.length;

      while (/\s/.test(source[cursor] ?? '')) {
        cursor += 1;
      }

      if (source[cursor] !== '=') {
        index += 1;
        continue;
      }

      cursor += 1;
      while (/\s/.test(source[cursor] ?? '')) {
        cursor += 1;
      }

      if (source[cursor] === '[') {
        return cursor;
      }
    }

    index += 1;
  }

  return -1;
}

function extractBalancedArrayLiteral(source, startIndex) {
  let depth = 0;
  let index = startIndex;

  while (index < source.length) {
    const char = source[index];
    const next = source[index + 1];

    if (char === '/' && (next === '/' || next === '*')) {
      index = skipComment(source, index);
      continue;
    }

    if (char === "'" || char === '"' || char === '`') {
      index = skipQuotedString(source, index, char);
      continue;
    }

    if (char === '[') {
      depth += 1;
    } else if (char === ']') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(startIndex, index + 1);
      }
    }

    index += 1;
  }

  throw new Error(`Unterminated array literal starting at ${startIndex}`);
}

function parseArrayLiteral(literal, variableName) {
  const result = vm.runInNewContext(`(${literal})`, Object.create(null), {
    timeout: 1_000,
  });

  if (!Array.isArray(result)) {
    throw new Error(`${variableName} did not evaluate to an array`);
  }

  return result;
}

function collectArrayEntries(source, variableName, kind) {
  const arrayStart = findArrayStart(source, variableName);
  if (arrayStart === -1) {
    return [];
  }

  const literal = extractBalancedArrayLiteral(source, arrayStart);
  const tuples = parseArrayLiteral(literal, variableName);

  return tuples
    .map((tuple) => {
      if (!Array.isArray(tuple) || tuple.length < 3) {
        return null;
      }

      const [url, region, description] = tuple;
      const normalizedUrl = normalizeText(url);

      if (!normalizedUrl.startsWith('http://') && !normalizedUrl.startsWith('https://')) {
        return null;
      }

      return {
        kind,
        url: normalizedUrl,
        region: normalizeText(region),
        description: normalizeText(description),
        source_array: variableName,
      };
    })
    .filter(Boolean);
}

function dedupeMirrors(entries) {
  const seen = new Set();
  const uniqueEntries = [];

  for (const entry of entries) {
    if (seen.has(entry.url)) {
      continue;
    }
    seen.add(entry.url);
    uniqueEntries.push(entry);
  }

  return uniqueEntries;
}

export function extractMirrorConfigFromUserscript(source) {
  const versionMatch = source.match(/@version\s+([^\s]+)/);
  const sourceVersion = versionMatch?.[1] ?? null;

  const download = dedupeMirrors([
    ...collectArrayEntries(source, 'download_url_us', 'download'),
    ...collectArrayEntries(source, 'download_url', 'download'),
  ]);

  const raw = dedupeMirrors(
    collectArrayEntries(source, 'raw_url', 'raw').filter(
      (entry) =>
        entry.url !== 'https://raw.githubusercontent.com' &&
        !entry.description.includes('Github 原生') &&
        !entry.region.includes('Github 原生'),
    ),
  );

  return {
    version: sourceVersion,
    download,
    raw,
  };
}

export function buildDownloadProbeUrl(mirrorUrl, originUrl = DOWNLOAD_PROBE_ORIGIN) {
  return originUrl.replace('https://github.com', mirrorUrl);
}

export function buildRawProbeUrl(mirrorUrl, originUrl = RAW_PROBE_ORIGIN) {
  if (mirrorUrl.endsWith('/gh') && !mirrorUrl.includes('cdn.staticaly.com')) {
    const rawUrl = new URL(originUrl);
    const [owner, repo, ref, ...pathParts] = rawUrl.pathname.split('/').filter(Boolean);
    if (!owner || !repo || !ref || pathParts.length === 0) {
      throw new Error(`Unsupported raw probe url: ${originUrl}`);
    }

    return `${mirrorUrl}/${owner}/${repo}@${ref}/${pathParts.join('/')}`;
  }

  return originUrl.replace('https://raw.githubusercontent.com', mirrorUrl);
}

function defaultHeaders(extraHeaders = {}) {
  return {
    'user-agent': USER_AGENT,
    ...extraHeaders,
  };
}

function hasSufficientContentLength(response) {
  const contentLength = response.headers.get('content-length');
  if (!contentLength) {
    return false;
  }

  const parsedLength = Number.parseInt(contentLength, 10);
  return Number.isFinite(parsedLength) && parsedLength >= MIN_DOWNLOAD_BYTES;
}

function looksLikeHtml(contentType, text) {
  if (contentType.toLowerCase().includes('text/html')) {
    return true;
  }

  const trimmed = text.trimStart().toLowerCase();
  return trimmed.startsWith('<!doctype html') || trimmed.startsWith('<html');
}

async function probeDownloadMirror(mirror, fetchImpl, timeoutMs) {
  const targetUrl = buildDownloadProbeUrl(mirror.url);
  let headFailureReason = null;

  try {
    const headResponse = await fetchImpl(targetUrl, {
      method: 'HEAD',
      headers: defaultHeaders(),
      redirect: 'follow',
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!headResponse.ok) {
      headFailureReason = `HEAD ${headResponse.status}`;
    } else if (hasSufficientContentLength(headResponse)) {
      return { ok: true };
    }
  } catch (error) {
    headFailureReason = error.message;
  }

  try {
    const getResponse = await fetchImpl(targetUrl, {
      headers: defaultHeaders({
        range: 'bytes=0-0',
      }),
      redirect: 'follow',
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!getResponse.ok) {
      return { ok: false, reason: `GET ${getResponse.status}` };
    }

    if (hasSufficientContentLength(getResponse)) {
      return { ok: true };
    }

    const contentType = getResponse.headers.get('content-type') ?? '';
    const buffer = new Uint8Array(await getResponse.arrayBuffer());

    if (buffer.byteLength > 0 && !looksLikeHtml(contentType, '')) {
      return { ok: true };
    }

    return {
      ok: false,
      reason: headFailureReason ?? 'download response did not look like a binary file',
    };
  } catch (error) {
    return { ok: false, reason: error.message ?? headFailureReason ?? 'fetch failed' };
  }
}

async function probeRawMirror(mirror, fetchImpl, timeoutMs) {
  const targetUrl = buildRawProbeUrl(mirror.url);

  try {
    const response = await fetchImpl(targetUrl, {
      headers: defaultHeaders(),
      redirect: 'follow',
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!response.ok) {
      return { ok: false, reason: `GET ${response.status}` };
    }

    const contentType = response.headers.get('content-type') ?? '';
    const text = await response.text();

    if (looksLikeHtml(contentType, text)) {
      return { ok: false, reason: 'raw mirror returned HTML' };
    }

    if (!text.includes(RAW_PROBE_EXPECTED_TEXT)) {
      return { ok: false, reason: 'raw mirror returned unexpected content' };
    }

    return { ok: true };
  } catch (error) {
    return { ok: false, reason: error.message };
  }
}

export async function filterWorkingMirrors(mirrors, probeMirror) {
  const active = [];
  const removed = [];

  for (const mirror of mirrors) {
    const result = await probeMirror(mirror);

    if (result.ok) {
      active.push(mirror);
      continue;
    }

    removed.push({
      ...mirror,
      reason: result.reason ?? 'unknown',
    });
  }

  return { active, removed };
}

export async function probeDownloadMirrorForTest(mirror, fetchImpl, timeoutMs) {
  return probeDownloadMirror(mirror, fetchImpl, timeoutMs);
}

export function buildOutputDocument({
  generatedAt,
  sourceVersion,
  download,
  raw,
}) {
  return {
    generated_at: generatedAt,
    source: {
      url: USERSCRIPT_BLOB_URL,
      raw_url: USERSCRIPT_RAW_URL,
      version: sourceVersion,
    },
    probe_targets: {
      download: DOWNLOAD_PROBE_ORIGIN,
      raw: RAW_PROBE_ORIGIN,
    },
    counts: {
      download: download.length,
      raw: raw.length,
    },
    download,
    raw,
  };
}

async function fetchUserscriptSource(fetchImpl, sourceUrl, timeoutMs) {
  const response = await fetchImpl(sourceUrl, {
    headers: defaultHeaders(),
    redirect: 'follow',
    signal: AbortSignal.timeout(timeoutMs),
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch userscript: ${response.status} ${response.statusText}`);
  }

  return response.text();
}

function parseCliArgs(argv) {
  const options = {
    outputPath: 'github_mirrors.json',
    sourceUrl: USERSCRIPT_RAW_URL,
    timeoutMs: DEFAULT_TIMEOUT_MS,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];

    if (arg === '--output') {
      options.outputPath = argv[index + 1];
      index += 1;
      continue;
    }

    if (arg === '--source-url') {
      options.sourceUrl = argv[index + 1];
      index += 1;
      continue;
    }

    if (arg === '--timeout-ms') {
      options.timeoutMs = Number.parseInt(argv[index + 1], 10);
      index += 1;
    }
  }

  return options;
}

export async function updateGithubMirrors({
  outputPath = 'github_mirrors.json',
  sourceUrl = USERSCRIPT_RAW_URL,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  fetchImpl = fetch,
  logger = console,
  generatedAt = new Date().toISOString(),
} = {}) {
  const source = await fetchUserscriptSource(fetchImpl, sourceUrl, timeoutMs);
  const extracted = extractMirrorConfigFromUserscript(source);

  if (extracted.download.length === 0) {
    throw new Error('No download mirrors were extracted from the userscript');
  }

  if (extracted.raw.length === 0) {
    throw new Error('No raw mirrors were extracted from the userscript');
  }

  logger.log(
    `Extracted ${extracted.download.length} download mirrors and ${extracted.raw.length} raw mirrors from userscript ${extracted.version ?? 'unknown'}.`,
  );

  const downloadProbe = (mirror) => probeDownloadMirror(mirror, fetchImpl, timeoutMs);
  const rawProbe = (mirror) => probeRawMirror(mirror, fetchImpl, timeoutMs);

  const { active: activeDownload, removed: removedDownload } = await filterWorkingMirrors(
    extracted.download,
    downloadProbe,
  );
  const { active: activeRaw, removed: removedRaw } = await filterWorkingMirrors(
    extracted.raw,
    rawProbe,
  );

  if (activeDownload.length === 0) {
    throw new Error('No working download mirrors were detected');
  }

  if (activeRaw.length === 0) {
    throw new Error('No working raw mirrors were detected');
  }

  const output = buildOutputDocument({
    generatedAt,
    sourceVersion: extracted.version,
    download: activeDownload,
    raw: activeRaw,
  });

  await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');

  if (removedDownload.length > 0 || removedRaw.length > 0) {
    logger.warn(
      `Removed ${removedDownload.length} download mirrors and ${removedRaw.length} raw mirrors after probing.`,
    );

    for (const mirror of [...removedDownload, ...removedRaw]) {
      logger.warn(`- ${mirror.kind}: ${mirror.url} (${mirror.reason})`);
    }
  }

  logger.log(`Wrote ${outputPath}`);

  return {
    output,
    removed: [...removedDownload, ...removedRaw],
  };
}

async function main() {
  const options = parseCliArgs(process.argv.slice(2));
  await updateGithubMirrors(options);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
