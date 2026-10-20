/* ==========================================================================
   Functional tests for prescription-list.js
   Runs with Node.js test runner: node --test website/components/prescription-list.test.js
   ========================================================================== */

const test = require('node:test');
const assert = require('node:assert/strict');

/* --------------------------------------------------------------------------
   Minimal DOM shim
   -------------------------------------------------------------------------- */

// The component validates `instanceof HTMLElement`, so the shim must be in
// place before the module is required.
global.HTMLElement = function HTMLElement() {};

function MockEl(dataset) {
  const el = this;
  el.dataset = dataset || {};
  el.tagName = 'DIV';
  el.className = '';
  el.innerHTML = '';
  el.children = [];
  el.parentNode = null;
  el._attributes = {};
  el._listeners = {};

  el.classList = {
    add(name) {
      if (!el.classList.contains(name)) {
        el.className = el.className ? `${el.className} ${name}` : name;
      }
    },
    remove(name) {
      el.className = el.className
        .split(/\s+/)
        .filter((cls) => cls && cls !== name)
        .join(' ');
    },
    contains(name) {
      return el.className.split(/\s+/).includes(name);
    },
  };
}

MockEl.prototype = Object.create(global.HTMLElement.prototype);
MockEl.prototype.constructor = MockEl;

MockEl.prototype.appendChild = function (child) {
  this.children.push(child);
  child.parentNode = this;
  return child;
};

MockEl.prototype.removeChild = function (child) {
  const index = this.children.indexOf(child);
  if (index !== -1) this.children.splice(index, 1);
  child.parentNode = null;
  return child;
};

MockEl.prototype.querySelector = function (selector) {
  if (typeof selector !== 'string') return null;
  const cls = selector.replace(/^\./, '');
  return this.children.find((child) => child.classList.contains(cls)) || null;
};

MockEl.prototype.querySelectorAll = function () {
  return [];
};

MockEl.prototype.setAttribute = function (name, value) {
  this._attributes[name] = String(value);
};

MockEl.prototype.getAttribute = function (name) {
  return Object.prototype.hasOwnProperty.call(this._attributes, name)
    ? this._attributes[name]
    : null;
};

MockEl.prototype.addEventListener = function (type, handler) {
  if (!this._listeners[type]) this._listeners[type] = [];
  this._listeners[type].push(handler);
};

function createElement(tag) {
  const el = new MockEl();
  el.tagName = String(tag).toUpperCase();
  return el;
}

global.document = {
  readyState: 'complete',
  querySelectorAll: () => [],
  addEventListener: () => {},
  createElement,
};
global.window = global;

const PrescriptionList = require('./prescription-list.js');

/* --------------------------------------------------------------------------
   Fixtures
   -------------------------------------------------------------------------- */

function makeRx(overrides) {
  return Object.assign(
    {
      id: 1,
      patient: 'GPATIENT000000000000000000000000000000000000000000000000',
      provider: 'GPROVIDER00000000000000000000000000000000000000000000000',
      lens_type: 'SingleVision',
      right_eye: {
        sphere: '-2.50',
        cylinder: '-0.75',
        axis: '180',
        add: '+1.00',
        pd: '32.0',
      },
      left_eye: {
        sphere: '-2.75',
        cylinder: '-0.50',
        axis: '175',
        add: '+1.00',
        pd: '31.5',
      },
    },
    overrides || {}
  );
}

function mountList(opts, dataset) {
  const mount = new MockEl(dataset);
  const list = new PrescriptionList(mount, opts);
  const root = mount.children[0];
  return { mount, list, root };
}

/* --------------------------------------------------------------------------
   Tests
   -------------------------------------------------------------------------- */

test('constructor rejects a non-HTMLElement mount', () => {
  assert.throws(() => new PrescriptionList(null, {}), TypeError);
  assert.throws(() => new PrescriptionList({}, {}), TypeError);
});

test('renders every prescription field for both eyes', () => {
  const { root } = mountList({ prescriptions: [makeRx()] });
  const html = root.innerHTML;

  for (const header of ['Sphere (SPH)', 'Cylinder (CYL)', 'Axis', 'Add Power', 'PD']) {
    assert.ok(html.includes(header), `should render ${header} column`);
  }

  assert.ok(html.includes('OD'), 'should label the right eye OD');
  assert.ok(html.includes('OS'), 'should label the left eye OS');
  assert.ok(html.includes('-2.50'), 'should render right eye sphere');
  assert.ok(html.includes('-2.75'), 'should render left eye sphere');
  assert.ok(html.includes('-0.75'), 'should render right eye cylinder');
  assert.ok(html.includes('-0.50'), 'should render left eye cylinder');
  assert.ok(html.includes('180'), 'should render right eye axis');
  assert.ok(html.includes('175'), 'should render left eye axis');
  assert.ok(html.includes('+1.00'), 'should render add power');
  assert.ok(html.includes('32.0'), 'should render right eye PD');
  assert.ok(html.includes('31.5'), 'should render left eye PD');
});

test('renders all required lens types', () => {
  const cases = [
    ['SingleVision', 'Single Vision', 'prescription-list__lens--single-vision'],
    ['Bifocal', 'Bifocal', 'prescription-list__lens--bifocal'],
    ['Progressive', 'Progressive', 'prescription-list__lens--progressive'],
    ['ContactLens', 'Contact Lens', 'prescription-list__lens--contact-lens'],
  ];

  for (const [input, label, cls] of cases) {
    const { root } = mountList({ prescriptions: [makeRx({ lens_type: input })] });
    assert.ok(root.innerHTML.includes(label), `should render ${label} label`);
    assert.ok(root.innerHTML.includes(cls), `should render ${label} badge class`);
  }
});

test('maps the contract Glasses lens type to Single Vision', () => {
  const { root } = mountList({ prescriptions: [makeRx({ lens_type: 'Glasses' })] });
  assert.ok(root.innerHTML.includes('Single Vision'), 'should map Glasses to Single Vision');
});

test('renders an em dash for missing prescription values', () => {
  const { root } = mountList({
    prescriptions: [makeRx({ right_eye: {}, left_eye: {} })],
  });
  assert.ok(root.innerHTML.includes('&mdash;'), 'should show a placeholder for missing values');
  assert.ok(root.innerHTML.includes('Not recorded'), 'should expose missing values accessibly');
});

test('paginates prescriptions and clamps at boundaries', () => {
  const prescriptions = Array.from({ length: 7 }, (_, index) => makeRx({ id: index + 1 }));
  const { list, root } = mountList({ prescriptions, pageSize: 3 });

  assert.ok(root.innerHTML.includes('Page 1 of 3'), 'should start on page 1');
  assert.ok(root.innerHTML.includes('Rx #1'), 'should render first item');
  assert.ok(!root.innerHTML.includes('Rx #4'), 'should not render page 2 items');
  assert.ok(root.innerHTML.includes('data-page-action="prev"'), 'should render previous control');
  assert.ok(root.innerHTML.includes('data-page-action="next"'), 'should render next control');

  list.nextPage();
  assert.ok(root.innerHTML.includes('Page 2 of 3'), 'should advance to page 2');
  assert.ok(root.innerHTML.includes('Rx #4'), 'should render page 2 items');
  assert.ok(!root.innerHTML.includes('Rx #1'), 'should drop page 1 items');

  list.nextPage();
  assert.ok(root.innerHTML.includes('Page 3 of 3'), 'should advance to page 3');
  assert.ok(root.innerHTML.includes('Rx #7'), 'should render the last item');

  list.nextPage();
  assert.ok(root.innerHTML.includes('Page 3 of 3'), 'should clamp at the last page');

  list.prevPage();
  assert.ok(root.innerHTML.includes('Page 2 of 3'), 'should go back one page');

  list.goToPage(99);
  assert.ok(root.innerHTML.includes('Page 3 of 3'), 'should clamp out-of-range pages');
});

test('hides pagination when everything fits on one page', () => {
  const { root } = mountList({ prescriptions: [makeRx(), makeRx({ id: 2 })], pageSize: 5 });
  assert.ok(!root.innerHTML.includes('prescription-list__pagination'), 'should hide pagination');
});

test('shows a loading state while fetching', async () => {
  const { list, root } = mountList({
    fetchPrescriptions: async () => [makeRx()],
  });

  const pending = list.load();
  assert.ok(root.classList.contains('is-loading'), 'should add is-loading class');
  assert.ok(root.innerHTML.includes('Loading prescriptions'), 'should show loading text');
  assert.equal(root.getAttribute('aria-busy'), 'true');

  await pending;

  assert.ok(!root.classList.contains('is-loading'), 'should clear is-loading class');
  assert.ok(root.innerHTML.includes('Rx #1'), 'should render fetched prescriptions');
  assert.equal(root.getAttribute('aria-busy'), 'false');
});

test('shows an error state when fetching fails', async () => {
  const { list, root } = mountList({
    fetchPrescriptions: async () => {
      throw new Error('clinical_records RPC unavailable');
    },
  });

  await list.load();

  assert.ok(root.innerHTML.includes('role="alert"'), 'should use an alert role');
  assert.ok(
    root.innerHTML.includes('clinical_records RPC unavailable'),
    'should show the provider error message'
  );
  assert.ok(!root.classList.contains('is-loading'), 'should not remain in loading state');
});

test('showError renders a caller-supplied message', () => {
  const { list, root } = mountList({ prescriptions: [makeRx()] });
  list.showError('Reader key rejected');
  assert.ok(root.innerHTML.includes('Reader key rejected'), 'should show the error message');
});

test('shows an empty state when no prescriptions are recorded', () => {
  const { root } = mountList({ prescriptions: [] });
  assert.ok(root.innerHTML.includes('No prescriptions recorded'), 'should show the empty message');
  assert.ok(!root.innerHTML.includes('prescription-list__pagination'), 'should hide pagination');
});

test('reads prescriptions, title and page size from data attributes', () => {
  const prescriptions = [makeRx({ id: 1 }), makeRx({ id: 2 }), makeRx({ id: 3 })];
  const { root } = mountList(
    {},
    {
      title: 'Contract Prescriptions',
      pageSize: '2',
      prescriptions: JSON.stringify(prescriptions),
    }
  );

  assert.ok(root.innerHTML.includes('Contract Prescriptions'), 'should read title');
  assert.ok(root.innerHTML.includes('Page 1 of 2'), 'should read page size');
});

test('escapes HTML from contract values', () => {
  const { root } = mountList({
    prescriptions: [
      makeRx({
        lens_type: '<img src=x onerror=alert(1)>',
        right_eye: {
          sphere: '<script>alert("xss")</script>',
          cylinder: '0.00',
          axis: '0',
          add: '0',
          pd: '0',
        },
      }),
    ],
  });

  assert.ok(!root.innerHTML.includes('<script>'), 'should not emit a script tag');
  assert.ok(root.innerHTML.includes('&lt;script&gt;'), 'should HTML-encode the script tag');
  assert.ok(!root.innerHTML.includes('<img src=x'), 'should not emit an image tag');
});

test('update() re-renders with new data and title', () => {
  const { list, root } = mountList({ prescriptions: [makeRx({ id: 1 })] });
  list.update({ title: 'Updated Prescriptions', prescriptions: [makeRx({ id: 9 })] });

  assert.ok(root.innerHTML.includes('Updated Prescriptions'), 'should render the new title');
  assert.ok(root.innerHTML.includes('Rx #9'), 'should render the new prescription');
  assert.ok(!root.innerHTML.includes('Rx #1'), 'should drop the old prescription');
});

test('destroy() removes the component from the DOM', () => {
  const { mount, list } = mountList({ prescriptions: [makeRx()] });
  assert.equal(mount.children.length, 1, 'should mount one root');

  list.destroy();

  assert.equal(mount.children.length, 0, 'should remove the root');
  assert.equal(list._mount, null, 'should clear the mount reference');
  assert.equal(list._root, null, 'should clear the root reference');
});
