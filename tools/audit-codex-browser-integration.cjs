#!/usr/bin/env node
'use strict';

// Read-only, version-pinned analysis of the installed JavaScript bundle.
// Node is intentional: replay the actual app functions instead of a Rust rewrite.
// This never starts the app, connects to its sockets, or reads user profiles.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const vm = require('node:vm');
const assert = require('node:assert/strict');

const VERSION = '26.1111.21308';
const PINS = {
  '.vite/build/early-bootstrap.js': '978e3858adec26fd8a7370cad2206a43a81b0be8799d17f4d53f833343a90e60',
  '.vite/build/early-bootstrap-226-2f6bd181558008cf.js': '97adba36c0c0e9e145ab364c82eb3433abe3675ab7c6e3ab282035b67a4d60bb',
  '.vite/build/early-bootstrap-631-0e6c54222e68c8ef.js': '67df539aae3e7e579da590bbefd074c70b537f977559168f273c875b21f13c55',
  'webview/assets/async/28918.48c9cffd9d.js': '17fbfa26256b8807539fb2762f9a636d02091999fc824c1904074354f8d31ca1',
};
const SERVICE_HASH = '0a1a265a6d57e53e6dd098e6d29514703a57baa11da4b063e549d3d6c13ff891';
const sha256 = (data) => crypto.createHash('sha256').update(data).digest('hex');

function readAt(fd, size, offset) {
  const bytes = Buffer.alloc(size);
  let read = 0;
  while (read < size) {
    const count = fs.readSync(fd, bytes, read, size - read, offset + read);
    if (count === 0) throw new Error('Truncated ASAR file');
    read += count;
  }
  return bytes;
}

function readArchive(file) {
  const fd = fs.openSync(file, 'r');
  try {
    const size = fs.fstatSync(fd).size;
    const prefix = readAt(fd, 16, 0);
    const headerSize = prefix.readUInt32LE(12);
    const start = 8 + prefix.readUInt32LE(4);
    assert(headerSize > 0 && headerSize <= 64 * 1024 * 1024, 'Invalid ASAR header size');
    assert(start >= 16 + headerSize && start <= size, 'Invalid ASAR data offset');
    const header = readAt(fd, headerSize, 16);
    const files = new Map();
    function walk(tree, parent = '') {
      for (const [name, entry] of Object.entries(tree)) {
        const nameInArchive = parent + name;
        if (entry.files) walk(entry.files, nameInArchive + '/');
        else files.set(nameInArchive, entry);
      }
    }
    walk(JSON.parse(header).files);
    function read(name) {
      const entry = files.get(name);
      assert(entry && !entry.link && !entry.unpacked, `Missing packed file: ${name}`);
      const offset = start + Number(entry.offset);
      assert(Number.isSafeInteger(offset) && offset >= start, 'Invalid file offset');
      assert(Number.isSafeInteger(entry.size) && entry.size >= 0 && entry.size <= 16 * 1024 * 1024, 'Invalid file size');
      assert(offset + entry.size <= size, 'ASAR entry is out of bounds');
      return readAt(fd, entry.size, offset);
    }
    const packageInfo = JSON.parse(read('package.json'));
    assert.equal(packageInfo.version, VERSION, 'Unsupported app version; inspect new bundles before updating the pins');
    const sources = Object.fromEntries(Object.entries(PINS).map(([name, expected]) => {
      const bytes = read(name);
      assert.equal(sha256(bytes), expected, `Bundle changed: ${name}`);
      return [name, bytes.toString('utf8')];
    }));
    return {
      packageInfo, sources, headerSha256: sha256(header),
      files: [...files.keys()],
    };
  } finally {
    fs.closeSync(fd);
  }
}

function sliceBetween(source, begin, end) {
  const start = source.indexOf(begin);
  assert(start >= 0 && source.indexOf(begin, start + begin.length) < 0, `Ambiguous/missing anchor: ${begin}`);
  const finish = source.indexOf(end, start + begin.length);
  assert(finish > start, `Missing end anchor: ${end}`);
  return source.slice(start, finish);
}

async function audit(root) {
  const archive = readArchive(path.join(root, 'resources/app.asar'));
  const [bootstrap, main, constants, settings] = Object.keys(PINS).map((name) => archive.sources[name]);
  const service = fs.readFileSync(path.join(root, 'resources/plugins/openai-bundled/plugins/browser/scripts/browser-service.mjs'));
  assert.equal(sha256(service), SERVICE_HASH, 'Browser service changed; review before using this audit');

  // Only the reviewed, pinned bundle is evaluated. vm is not a security boundary
  // for arbitrary files. Remove the entrypoint, retain the module loader, and
  // call only the pure catalog/Unicode/protocol-constant factories.
  const startup = 'var o=r(10478);module.exports=o';
  assert.equal(bootstrap.split(startup).length, 2, 'Unexpected app entrypoint');
  const context = vm.createContext({ module: { exports: {} }, exports: {} }, {
    codeGeneration: { strings: false, wasm: false },
  });
  const evaluate = (code) => vm.runInContext(code, context, { timeout: 1000 });
  evaluate(bootstrap.replace(startup, 'module.exports={modules:a,load:r}'));
  evaluate(constants); // Registers factories; does not run app services.
  evaluate('Object.assign(module.exports.modules,exports.modules); var catalog=module.exports.load(79895); var lj=module.exports.load(99970); var lU=module.exports.load(12200); var lH=module.exports.load(88933);');

  const discovery = sliceBetween(main, 'function MR(){', 'var MA=r(1229)');
  const stringValidator = sliceBetween(main, 'function lB(e){', 'function lW(e)');
  // Replay the full installed discovery function. All I/O is synthetic and all
  // fixtures are local data. Even the official-ID positive control opens no socket.
  evaluate(`
    var n5=catalog, eY={qr1:catalog.qr};
    var e9=()=>({platform:()=>"linux"}), tb=()=>({getuid:()=>1000});
    var eQ=()=>({join:(...p)=>p.join("/")}), lY="unused-windows-pipe";
    var tr=()=>({readdir:async()=>[{name:"fixture.sock"}],lstat:async()=>({isSocket:()=>true,uid:1000})});
    var descriptor, requests=0, closes=0;
    var lZ={connect:async()=>({getInfo:async()=>{requests++;return descriptor;},close:async()=>{closes++;}})};
    ${stringValidator}
    ${discovery}
    var runDiscovery=MR();
  `);

  const families = JSON.parse(evaluate('JSON.stringify(Object.keys(catalog.Yp))'));
  const extensionIds = JSON.parse(evaluate('JSON.stringify(catalog.qr("prod"))'));
  const acceptedFamily = JSON.parse(evaluate('JSON.stringify(Object.fromEntries(["chrome","edge","brave","opera","vivaldi","firefox","zen"].map(f=>[f,catalog.rU(f)])))'));
  const ownId = 'firefox-codex-mcp@local.invalid';
  const cases = [
    ['reference_chrome', 'chrome', extensionIds[0], 'prod', true],
    ['firefox_own_id', 'firefox', ownId, 'prod', false],
    ['firefox_reference_id', 'firefox', extensionIds[0], 'prod', false],
    ['chrome_gecko_id', 'chrome', ownId, 'prod', false],
    ['reference_wrong_channel', 'chrome', extensionIds[0], 'dev', false],
  ];
  const replay = [];
  for (const [name, family, extensionId, buildChannel, expected] of cases) {
    context.descriptor = {
      type: 'extension', family,
      metadata: { extensionId, extensionInstanceId: 'audit-fixture-instance' },
      browserTabMentions: { protocolVersion: evaluate('lU.yG'), buildChannel, generation: 'audit-fixture-generation' },
    };
    evaluate('requests=0;closes=0;');
    const result = await evaluate('runDiscovery()');
    const accepted = result.length === 1;
    assert.equal(accepted, expected, `Unexpected discovery outcome: ${name}`);
    assert.equal(context.requests, 1, 'Discovery did not call getInfo');
    assert.equal(context.closes, accepted ? 0 : 1, 'Rejected fixture was not closed');
    replay.push({ name, accepted, rejectedConnectionClosed: context.closes === 1 });
  }
  context.geckoId = ownId;
  const geckoIdResult = evaluate('(()=>{try{catalog.qr("prod",[geckoId]);return {accepted:true};}catch(e){return {accepted:false,error:e.message};}})()');
  assert.equal(geckoIdResult.accepted, false, 'Gecko extension-ID handling changed');
  const settingsCatalogAnchor = 'Object.keys(c.Ypn).filter(c.rUV)';
  assert(settings.includes(settingsCatalogAnchor), 'Settings catalog path changed');
  const cdpMethods = ['Page.captureScreenshot', 'Accessibility.getFullAXTree', 'DOM.getFrameOwner', 'Input.dispatchMouseEvent'];
  assert(cdpMethods.every((method) => service.includes(method)), 'Expected CDP dependency changed');

  return {
    scope: 'Read-only installed-bundle replay with synthetic I/O; NOT desktop UI or browser E2E.',
    app: { name: archive.packageInfo.name, version: archive.packageInfo.version, archiveHeaderSha256: archive.headerSha256 },
    sourceAvailability: {
      archiveFileCount: archive.files.length,
      sourceMapCount: archive.files.filter((file) => file.endsWith('.map')).length,
      appTypeScriptOrRustCount: archive.files.filter((file) => !file.includes('node_modules/') && /\.(tsx?|rs)$/.test(file)).length,
    },
    families, acceptedFamily, productionExtensionIds: extensionIds,
    nativeDiscoveryReplay: replay,
    additionalGeckoId: geckoIdResult,
    settings: { enumeratesFixedCatalog: true, catalogByteOffset: Buffer.byteLength(settings.slice(0, settings.indexOf(settingsCatalogAnchor))) },
    controlService: { cdpMethods, sha256: SERVICE_HASH },
    verifiedBundleSha256: PINS,
    conclusion: 'This build rejects a distinct Firefox provider. MCP/dynamicTools registration does not add a Settings family. App changes plus a compatible control backend are required.',
  };
}

if (process.argv.includes('--help')) {
  process.stdout.write('Usage: node tools/audit-codex-browser-integration.cjs [installed-app-directory]\nDefault: /usr/lib/chatgpt. Read-only; supports only the pinned app build. JSON goes to stdout.\n');
} else if (process.argv.length > 3) {
  process.stderr.write('Expected at most one installed-app-directory argument.\n');
  process.exitCode = 1;
} else {
  audit(path.resolve(process.argv[2] || '/usr/lib/chatgpt')).then(
    (report) => process.stdout.write(JSON.stringify(report, null, 2) + '\n'),
    (error) => { process.stderr.write(`Audit failed: ${error.message}\n`); process.exitCode = 1; },
  );
}
