import test from 'node:test';
import assert from 'node:assert/strict';

import {
  USERSCRIPT_BLOB_URL,
  buildDownloadProbeUrl,
  buildOutputDocument,
  buildRawProbeUrl,
  extractMirrorConfigFromUserscript,
  filterWorkingMirrors,
  probeDownloadMirrorForTest,
} from '../update_github_mirrors.mjs';

test('extractMirrorConfigFromUserscript handles merged download arrays and skips direct raw source', () => {
  const source = `
// @version      2.6.34
const download_url_us = [
  ['https://download-us.example/https://github.com', '美国', '[美国] 第一条'],
//], download_url = [
  ['https://download-other.example/https://github.com', '其他', '[其他] 第二条&#10;第二行说明'],
], raw_url = [
  ['https://raw.githubusercontent.com', 'Github 原生', '[官方 Raw]'],
  ['https://raw-mirror.example/https://raw.githubusercontent.com', '香港 1', '[Raw 镜像]'],
  ['https://fastly.jsdelivr.net/gh', '日本 1', '[JSDelivr CDN]'],
];
`;

  const config = extractMirrorConfigFromUserscript(source);

  assert.equal(config.version, '2.6.34');
  assert.deepEqual(
    config.download.map((mirror) => mirror.url),
    [
      'https://download-us.example/https://github.com',
      'https://download-other.example/https://github.com',
    ],
  );
  assert.equal(config.download[1].description, '[其他] 第二条\n第二行说明');
  assert.deepEqual(
    config.raw.map((mirror) => mirror.url),
    [
      'https://raw-mirror.example/https://raw.githubusercontent.com',
      'https://fastly.jsdelivr.net/gh',
    ],
  );
});

test('extractMirrorConfigFromUserscript merges optional download_url array when it exists separately', () => {
  const source = `
const download_url_us = [
  ['https://download-us.example/https://github.com', '美国', 'US'],
], download_url = [
  ['https://download-other.example/https://github.com', '其他', 'Other'],
], raw_url = [
  ['https://raw.githubusercontent.com', 'Github 原生', 'Direct'],
  ['https://raw-mirror.example/https://raw.githubusercontent.com', '香港', 'Raw'],
];
`;

  const config = extractMirrorConfigFromUserscript(source);

  assert.deepEqual(
    config.download.map((mirror) => mirror.url),
    [
      'https://download-us.example/https://github.com',
      'https://download-other.example/https://github.com',
    ],
  );
});

test('build probe urls for download and special raw mirrors', () => {
  assert.equal(
    buildDownloadProbeUrl('https://down.npee.cn/?https://github.com'),
    'https://down.npee.cn/?https://github.com/XIU2/CloudflareSpeedTest/releases/download/v2.2.2/CloudflareST_windows_amd64.zip',
  );
  assert.equal(
    buildRawProbeUrl('https://fastly.jsdelivr.net/gh'),
    'https://fastly.jsdelivr.net/gh/XIU2/UserScript@master/GithubEnhanced-High-Speed-Download.user.js',
  );
  assert.equal(
    buildRawProbeUrl('https://raw-mirror.example/https://raw.githubusercontent.com'),
    'https://raw-mirror.example/https://raw.githubusercontent.com/XIU2/UserScript/master/GithubEnhanced-High-Speed-Download.user.js',
  );
});

test('filterWorkingMirrors keeps only usable mirrors and records failures', async () => {
  const mirrors = [
    {
      kind: 'download',
      url: 'https://ok.example/https://github.com',
      region: '美国',
      description: 'ok',
      source_array: 'download_url_us',
    },
    {
      kind: 'download',
      url: 'https://bad.example/https://github.com',
      region: '美国',
      description: 'bad',
      source_array: 'download_url_us',
    },
  ];

  const { active, removed } = await filterWorkingMirrors(
    mirrors,
    async (mirror) => {
      if (mirror.url.includes('ok.example')) {
        return { ok: true };
      }
      return { ok: false, reason: 'timeout' };
    },
  );

  assert.deepEqual(active.map((mirror) => mirror.url), ['https://ok.example/https://github.com']);
  assert.deepEqual(removed, [
    {
      kind: 'download',
      url: 'https://bad.example/https://github.com',
      region: '美国',
      description: 'bad',
      source_array: 'download_url_us',
      reason: 'timeout',
    },
  ]);
});

test('probeDownloadMirror falls back to GET when HEAD is rejected', async () => {
  const methods = [];
  const mirror = {
    kind: 'download',
    url: 'https://download-us.example/https://github.com',
    region: '美国',
    description: 'desc',
    source_array: 'download_url_us',
  };

  const result = await probeDownloadMirrorForTest(
    mirror,
    async (_url, init = {}) => {
      const method = init.method ?? 'GET';
      methods.push(method);

      if (method === 'HEAD') {
        return new Response('', {
          status: 403,
          headers: {
            'content-type': 'text/html; charset=utf-8',
          },
        });
      }

      return new Response(new Uint8Array([1]), {
        status: 206,
        headers: {
          'content-type': 'application/octet-stream',
        },
      });
    },
    1_000,
  );

  assert.deepEqual(methods, ['HEAD', 'GET']);
  assert.deepEqual(result, { ok: true });
});

test('buildOutputDocument returns stable json-friendly structure', () => {
  const generatedAt = '2026-04-04T00:00:00.000Z';
  const document = buildOutputDocument({
    generatedAt,
    sourceVersion: '2.6.34',
    download: [
      {
        kind: 'download',
        url: 'https://download-us.example/https://github.com',
        region: '美国',
        description: 'desc',
        source_array: 'download_url_us',
      },
    ],
    raw: [
      {
        kind: 'raw',
        url: 'https://raw-mirror.example/https://raw.githubusercontent.com',
        region: '香港',
        description: 'desc',
        source_array: 'raw_url',
      },
    ],
    removed: [],
  });

  assert.equal(document.generated_at, generatedAt);
  assert.equal(document.source.url, USERSCRIPT_BLOB_URL);
  assert.equal(document.source.version, '2.6.34');
  assert.equal(document.counts.download, 1);
  assert.equal(document.counts.raw, 1);
  assert.equal(document.download[0].kind, 'download');
  assert.equal(document.raw[0].kind, 'raw');
});
