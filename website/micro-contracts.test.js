/* ==========================================================================
   Micro-Contracts Registry & Action Router Tests (Issue #5)
   Runs with Node.js test runner: node --test website/micro-contracts.test.js
   ========================================================================== */

const test = require('node:test');
const assert = require('node:assert/strict');

// Import app.js exports
const {
  DEFAULT_CONTRACT_REGISTRY,
  DEFAULT_ACTION_ROUTES,
  ContractRegistry,
  ContractRouter,
  SorobanRPC,
  truncateMiddle,
} = require('./app.js');

test('ContractRegistry — initialization and defaults', async (t) => {
  ContractRegistry.reset();

  await t.test('contains all required micro-contracts', () => {
    const requiredContracts = ['audit', 'identity', 'key_manager', 'vision_records'];
    for (const name of requiredContracts) {
      assert.ok(ContractRegistry.has(name), `Missing contract: ${name}`);
      const address = ContractRegistry.get(name);
      assert.ok(
        typeof address === 'string' && address.length > 10,
        `Invalid address for ${name}: ${address}`
      );
    }
  });

  await t.test('has default contract addresses matching DEFAULT_CONTRACT_REGISTRY', () => {
    const all = ContractRegistry.getAll();
    assert.strictEqual(all.audit, DEFAULT_CONTRACT_REGISTRY.audit);
    assert.strictEqual(all.identity, DEFAULT_CONTRACT_REGISTRY.identity);
    assert.strictEqual(all.key_manager, DEFAULT_CONTRACT_REGISTRY.key_manager);
    assert.strictEqual(all.vision_records, DEFAULT_CONTRACT_REGISTRY.vision_records);
  });
});

test('ContractRegistry — address management and validation', async (t) => {
  ContractRegistry.reset();

  await t.test('updates address via set() and normalizes key case', () => {
    const customAddr = 'CAUDIT_CUSTOM_TEST_ADDRESS_1234567890';
    ContractRegistry.set('AUDIT', customAddr);
    assert.strictEqual(ContractRegistry.get('audit'), customAddr);
    assert.strictEqual(ContractRegistry.get('AUDIT'), customAddr);
  });

  await t.test('throws TypeError on invalid name or address in set()', () => {
    assert.throws(() => ContractRegistry.set('', 'CVALIDADDRESS'), { name: 'TypeError' });
    assert.throws(() => ContractRegistry.set(null, 'CVALIDADDRESS'), { name: 'TypeError' });
    assert.throws(() => ContractRegistry.set('identity', ''), { name: 'TypeError' });
    assert.throws(() => ContractRegistry.set('identity', 12345), { name: 'TypeError' });
  });

  await t.test('throws Error when getting unregistered micro-contract', () => {
    assert.throws(() => ContractRegistry.get('non_existent_module'), {
      name: 'Error',
      message: /not registered/i,
    });
  });

  await t.test('loads batch config overrides via loadFromConfig()', () => {
    ContractRegistry.loadFromConfig({
      audit: 'C_BATCH_AUDIT_ADDRESS',
      key_manager: 'C_BATCH_KEY_MGR_ADDRESS',
    });
    assert.strictEqual(ContractRegistry.get('audit'), 'C_BATCH_AUDIT_ADDRESS');
    assert.strictEqual(ContractRegistry.get('key_manager'), 'C_BATCH_KEY_MGR_ADDRESS');
    // vision_records should remain intact
    assert.strictEqual(
      ContractRegistry.get('vision_records'),
      DEFAULT_CONTRACT_REGISTRY.vision_records
    );
  });

  await t.test('reset() restores original defaults', () => {
    ContractRegistry.reset();
    assert.strictEqual(ContractRegistry.get('audit'), DEFAULT_CONTRACT_REGISTRY.audit);
    assert.strictEqual(ContractRegistry.get('key_manager'), DEFAULT_CONTRACT_REGISTRY.key_manager);
  });
});

test('ContractRouter — action to micro-contract mapping', async (t) => {
  ContractRouter.reset();

  await t.test('routes vision_records actions correctly', () => {
    const actions = ['grant_access', 'revoke_access', 'get_record', 'rbac-grant', 'data-fetch'];
    for (const action of actions) {
      assert.strictEqual(ContractRouter.getContractForAction(action), 'vision_records');
    }
  });

  await t.test('routes identity actions correctly', () => {
    const actions = [
      'register_patient',
      'verify_identity',
      'get_profile',
      'identity-verify',
      'patient-auth',
    ];
    for (const action of actions) {
      assert.strictEqual(ContractRouter.getContractForAction(action), 'identity');
    }
  });

  await t.test('routes key_manager actions correctly', () => {
    const actions = ['rotate_key', 'store_key', 'get_public_key', 'revoke_key', 'key-rotation'];
    for (const action of actions) {
      assert.strictEqual(ContractRouter.getContractForAction(action), 'key_manager');
    }
  });

  await t.test('routes audit actions correctly', () => {
    const actions = ['log_event', 'get_audit_trail', 'verify_audit', 'audit-log'];
    for (const action of actions) {
      assert.strictEqual(ContractRouter.getContractForAction(action), 'audit');
    }
  });

  await t.test('routes directly when action name is a registered contract', () => {
    assert.strictEqual(ContractRouter.getContractForAction('audit'), 'audit');
    assert.strictEqual(ContractRouter.getContractForAction('identity'), 'identity');
    assert.strictEqual(ContractRouter.getContractForAction('vision_records'), 'vision_records');
  });

  await t.test('throws Error on unregistered action', () => {
    assert.throws(() => ContractRouter.getContractForAction('completely_unknown_action'), {
      name: 'Error',
      message: /No micro-contract route registered/i,
    });
  });

  await t.test('registers custom route dynamically', () => {
    ContractRouter.registerRoute('audit_scan_access', 'audit');
    assert.strictEqual(ContractRouter.getContractForAction('audit_scan_access'), 'audit');
  });

  await t.test('routeAction resolves contract address from registry', () => {
    ContractRegistry.reset();
    const routed = ContractRouter.routeAction('rbac-grant');
    assert.strictEqual(routed.action, 'rbac-grant');
    assert.strictEqual(routed.microContract, 'vision_records');
    assert.strictEqual(routed.contractAddress, DEFAULT_CONTRACT_REGISTRY.vision_records);

    const auditRoute = ContractRouter.routeAction('log_event');
    assert.strictEqual(auditRoute.microContract, 'audit');
    assert.strictEqual(auditRoute.contractAddress, DEFAULT_CONTRACT_REGISTRY.audit);
  });
});

test('SorobanRPC — payload building and invocation routing', async (t) => {
  ContractRegistry.reset();
  ContractRouter.reset();
  SorobanRPC.clearHistory();

  await t.test('buildPayload constructs standard JSON-RPC 2.0 structure', () => {
    const payload = SorobanRPC.buildPayload('grant_access', { doctor: 'GDR123', ttl: 3600 });
    assert.strictEqual(payload.jsonrpc, '2.0');
    assert.strictEqual(payload.method, 'simulateTransaction');
    assert.strictEqual(payload.params.microContract, 'vision_records');
    assert.strictEqual(payload.params.contractAddress, DEFAULT_CONTRACT_REGISTRY.vision_records);
    assert.strictEqual(payload.params.functionName, 'grant_access');
    assert.deepStrictEqual(payload.params.args, { doctor: 'GDR123', ttl: 3600 });
  });

  await t.test('invoke routes to key_manager micro-contract successfully', async () => {
    const res = await SorobanRPC.invoke('rotate_key', { keyId: 'key_primary' }, { delay: 10 });
    assert.strictEqual(res.status, 'success');
    assert.strictEqual(res.microContract, 'key_manager');
    assert.strictEqual(res.contractAddress, DEFAULT_CONTRACT_REGISTRY.key_manager);
    assert.ok(res.result.txHash.startsWith('0x'));
  });

  await t.test('invoke routes to audit micro-contract successfully', async () => {
    const res = await SorobanRPC.invoke(
      'log_event',
      { eventType: 'LOGIN', user: 'GDC1' },
      { delay: 10 }
    );
    assert.strictEqual(res.status, 'success');
    assert.strictEqual(res.microContract, 'audit');
    assert.strictEqual(res.contractAddress, DEFAULT_CONTRACT_REGISTRY.audit);
  });

  await t.test('invoke routes to identity micro-contract successfully', async () => {
    const res = await SorobanRPC.invoke('verify_identity', { patientId: 'PID_99' }, { delay: 10 });
    assert.strictEqual(res.status, 'success');
    assert.strictEqual(res.microContract, 'identity');
    assert.strictEqual(res.contractAddress, DEFAULT_CONTRACT_REGISTRY.identity);
  });

  await t.test('records invocation history accurately', () => {
    const history = SorobanRPC.getHistory();
    assert.ok(history.length >= 3);
    const last = history[history.length - 1];
    assert.strictEqual(last.action, 'verify_identity');
    assert.strictEqual(last.microContract, 'identity');
    assert.strictEqual(last.status, 'success');
  });

  await t.test('handles simulated contract revert', async () => {
    const res = await SorobanRPC.invoke('grant_access', {}, { delay: 10, simulateFailure: true });
    assert.strictEqual(res.status, 'error');
    assert.ok(res.error.includes('reverted'));
  });
});

test('truncateMiddle utility helper', async (t) => {
  await t.test('shortens long addresses with ellipsis in middle', () => {
    const full = 'CAUDIT7XX4M5K6RETINAXAUDITTRAIL2026PROTOCOLLOGGER00001';
    const truncated = truncateMiddle(full, 6, 6);
    assert.strictEqual(truncated, 'CAUDIT...R00001');
  });

  await t.test('returns full string if shorter than cutoff', () => {
    assert.strictEqual(truncateMiddle('SHORT', 6, 6), 'SHORT');
  });

  await t.test('handles non-string inputs safely', () => {
    assert.strictEqual(truncateMiddle(null), '');
    assert.strictEqual(truncateMiddle(undefined), '');
  });
});
