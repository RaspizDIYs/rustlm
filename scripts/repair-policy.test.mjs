import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import ts from "typescript";

const source = readFileSync(
  new URL("../src/lib/repair-policy.ts", import.meta.url),
  "utf8",
);
const javascript = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;
const { lostRepairServices, repairSignals, dnsRepairPins, nextRepairVariant } = await import(
  `data:text/javascript;base64,${Buffer.from(javascript).toString("base64")}`
);
const probe = (overrides = {}) => ({
  domain: "auth.riotgames.com",
  ip: "104.16.119.50",
  port: 443,
  request_path: "/",
  tcp: true,
  tls: true,
  classification: "http_success",
  http: 200,
  ...overrides,
});
const report = (overrides = {}) => ({
  context: { connection: "zapret", region: "EUW" },
  dns: [],
  probes: [],
  cancelled: false,
  discovery_limited: false,
  ...overrides,
});

test("Cloudflare refusal and login/2FA never trigger TLS tuning", () => {
  const blocked = report({
    probes: [probe({ classification: "cloudflare_block", http: 403 })],
  });
  assert.equal(repairSignals(blocked).cloudflareBlocks, 1);
  assert.equal(nextRepairVariant(0, blocked, "failed"), null);
  const tls = report({ probes: [probe({ tls: false })] });
  assert.equal(nextRepairVariant(0, tls, "login_required"), null);
});
test("one working IPv4 prevents a false failure from IPv6 or an alternate IP", () => {
  const result = report({
    probes: [
      probe(),
      probe({ ip: "104.16.120.50", tls: false }),
      probe({ ip: "2606:4700::1234", tcp: false, tls: false }),
    ],
  });
  assert.equal(repairSignals(result).tlsFailures, 0);
  assert.equal(nextRepairVariant(0, result, "failed"), null);
});
test("chat is checked on 5223 and another region's lobby does not retune EUW", () => {
  const result = report({
    probes: [
      probe({ domain: "euw1.chat.si.riotgames.com", port: 443, tls: false }),
      probe({ domain: "euw1.chat.si.riotgames.com", port: 5223 }),
      probe({ domain: "feapp.la1.lol.pvp.net", port: 2099, tls: false }),
    ],
  });
  assert.equal(repairSignals(result).tlsFailures, 0);
});
test("automatic TLS tuning has a strict three-candidate limit", () => {
  const result = report({ probes: [probe({ tls: false })] });
  assert.equal(nextRepairVariant(0, result, "failed"), 1);
  assert.equal(nextRepairVariant(1, result, "failed"), 2);
  assert.equal(nextRepairVariant(2, result, "failed"), null);
  assert.equal(
    nextRepairVariant(0, { ...result, cancelled: true }, "failed"),
    null,
  );
  assert.equal(
    nextRepairVariant(0, { ...result, discovery_limited: true }, "failed"),
    null,
  );
});
test("hosts changes require broken system DNS and successful pinned HTTPS without VPN", () => {
  const result = report({
    probes: [probe()],
    dns: [
      { domain: "auth.riotgames.com", resolver: "system", addresses: [] },
      {
        domain: "auth.riotgames.com",
        resolver: "google",
        addresses: ["104.16.119.50"],
      },
    ],
  });
  assert.deepEqual(dnsRepairPins(result), [
    { domain: "auth.riotgames.com", ip: "104.16.119.50" },
  ]);
  assert.deepEqual(
    dnsRepairPins({
      ...result,
      context: { ...result.context, connection: "vpn" },
    }),
    [],
  );
  assert.deepEqual(
    dnsRepairPins({
      ...result,
      probes: [probe({ http: 403, classification: "cloudflare_block" })],
    }),
    [],
  );
  assert.deepEqual(
    dnsRepairPins({
      ...result,
      dns: [
        {
          domain: "auth.riotgames.com",
          resolver: "system",
          addresses: ["104.16.119.50"],
        },
      ],
    }),
    [],
  );
});

test("regression compares IPv4 services across changing addresses", () => {
  const before = report({probes:[probe(), probe({ip:"::1"})]});
  assert.deepEqual(lostRepairServices(before, report({probes:[probe({ip:"1.1.1.1"})]})), []);
  assert.deepEqual(lostRepairServices(before, report({probes:[probe({tls:false})]})), ["auth.riotgames.com:443/"]);
  assert.deepEqual(lostRepairServices(before, report({probes:[probe({tls:false}),probe({ip:"1.1.1.1"})]})), []);
  assert.deepEqual(lostRepairServices(before, report()), []);
});
