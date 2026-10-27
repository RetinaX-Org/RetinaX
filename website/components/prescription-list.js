/* ==========================================================================
   PrescriptionList — Standalone UI Component Module
   RetinaX Design System: Clinical White · Slate Navy · Medical Teal

   Displays patient prescriptions read from the clinical_records contract.
   Expects the contract's Prescription shape:
     { id, lens_type, left_eye, right_eye }
   where each eye is { sphere, cylinder, axis, add, pd }.

   Usage (HTML attribute API):
   --------------------------------------------------------------------------
   <div class="prescription-list-mount"
        data-title="Patient Prescriptions"
        data-page-size="5"
        data-prescriptions='[
          {"id":1,"lens_type":"SingleVision",
           "right_eye":{"sphere":"-2.50","cylinder":"-0.75","axis":"180","add":"+1.00","pd":"32.0"},
           "left_eye":{"sphere":"-2.75","cylinder":"-0.50","axis":"175","add":"+1.00","pd":"31.5"}}
        ]'
   ></div>
   --------------------------------------------------------------------------
   JS Programmatic API:
   --------------------------------------------------------------------------
   const list = new PrescriptionList(mountEl, {
     title: 'Patient Prescriptions',
     pageSize: 5,
     prescriptions: [ ...contract Prescription objects... ],
     fetchPrescriptions: async () => [ ...contract Prescription objects... ],
   });

   // When using fetchPrescriptions, call load() to fetch and render:
   await list.load();

   list.nextPage();
   list.prevPage();
   list.goToPage(2);
   list.showLoading();
   list.showError('Unable to read clinical_records.');
   list.destroy();
   --------------------------------------------------------------------------
   ========================================================================== */

(function (root, factory) {
  /* UMD export — works as ES module, CommonJS, or plain <script> */
  if (typeof define === 'function' && define.amd) {
    define([], factory);
  } else if (typeof module === 'object' && module.exports) {
    module.exports = factory();
  } else {
    root.PrescriptionList = factory();
  }
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  /* ------------------------------------------------------------------
     Constants & Helpers
     ------------------------------------------------------------------ */

  const DEFAULT_PAGE_SIZE = 5;
  const DEFAULT_TITLE = 'Prescriptions';

  /**
   * Lens type display map.
   * The issue requires SingleVision, Bifocal, Progressive and ContactLens.
   * The clinical_records LensType enum currently exposes Glasses and
   * ContactLens, so Glasses is treated as Single Vision.
   */
  const LENS_TYPES = {
    singlevision: {
      label: 'Single Vision',
      cls: 'prescription-list__lens--single-vision',
    },
    glasses: {
      label: 'Single Vision',
      cls: 'prescription-list__lens--single-vision',
    },
    bifocal: {
      label: 'Bifocal',
      cls: 'prescription-list__lens--bifocal',
    },
    progressive: {
      label: 'Progressive',
      cls: 'prescription-list__lens--progressive',
    },
    contactlens: {
      label: 'Contact Lens',
      cls: 'prescription-list__lens--contact-lens',
    },
  };

  /**
   * Escape HTML to prevent XSS from contract values.
   * @param {*} value
   * @returns {string}
   */
  function escHtml(value) {
    if (value === null || value === undefined) return '';
    return String(value)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  /**
   * Parse a JSON array or return an array as-is.
   * @param {Array|string} raw
   * @returns {Array}
   */
  function parseArray(raw) {
    if (!raw) return [];
    if (Array.isArray(raw)) return raw;
    if (typeof raw === 'string') {
      try {
        const parsed = JSON.parse(raw);
        return Array.isArray(parsed) ? parsed : [];
      } catch (_) {
        return [];
      }
    }
    return [];
  }

  /**
   * Resolve a lens type to its display label and badge class.
   * @param {string} value
   * @returns {{label: string, cls: string}}
   */
  function normalizeLensType(value) {
    const raw = value === null || value === undefined ? '' : String(value);
    const key = raw.replace(/[\s_-]+/g, '').toLowerCase();
    if (LENS_TYPES[key]) return LENS_TYPES[key];
    return {
      label: raw || 'Unspecified',
      cls: 'prescription-list__lens--unknown',
    };
  }

  /**
   * Render a prescription value, or an em dash when not recorded.
   * @param {*} value
   * @returns {string}
   */
  function renderValue(value) {
    if (value === null || value === undefined || value === '') {
      return '<span class="prescription-list__value--empty" aria-label="Not recorded">&mdash;</span>';
    }
    return escHtml(value);
  }

  /**
   * Build one eye row of the prescription table.
   * @param {string} code  OD / OS
   * @param {string} name  Right Eye / Left Eye
   * @param {Object} data  PrescriptionData
   * @returns {string}
   */
  function buildEyeRow(code, name, data) {
    const eye = data || {};
    return `
      <tr class="prescription-list__row">
        <th scope="row" class="prescription-list__eye">
          <span class="prescription-list__eye-code">${code}</span>
          <span class="prescription-list__eye-label">${name}</span>
        </th>
        <td class="prescription-list__value">${renderValue(eye.sphere)}</td>
        <td class="prescription-list__value">${renderValue(eye.cylinder)}</td>
        <td class="prescription-list__value">${renderValue(eye.axis)}</td>
        <td class="prescription-list__value">${renderValue(eye.add)}</td>
        <td class="prescription-list__value">${renderValue(eye.pd)}</td>
      </tr>
    `;
  }

  /**
   * Build a single prescription card.
   * @param {Object} rx          Contract Prescription object
   * @param {number} fallbackId  Position used when the contract id is missing
   * @returns {string}
   */
  function buildItemHTML(rx, fallbackId) {
    const lens = normalizeLensType(rx.lens_type !== undefined ? rx.lens_type : rx.lensType);
    const id = rx.id !== undefined && rx.id !== null ? rx.id : fallbackId;
    const rightEye = rx.right_eye || rx.rightEye || {};
    const leftEye = rx.left_eye || rx.leftEye || {};

    return `
      <article class="prescription-list__item" aria-label="Prescription ${escHtml(id)}">
        <header class="prescription-list__item-header">
          <span class="prescription-list__rx-id">Rx #${escHtml(id)}</span>
          <span class="prescription-list__lens ${lens.cls}">${escHtml(lens.label)}</span>
        </header>
        <div class="prescription-list__table-wrap">
          <table class="prescription-list__table">
            <caption class="prescription-list__caption">Prescription values by eye</caption>
            <thead>
              <tr>
                <th scope="col">Eye</th>
                <th scope="col">Sphere (SPH)</th>
                <th scope="col">Cylinder (CYL)</th>
                <th scope="col">Axis</th>
                <th scope="col">Add Power</th>
                <th scope="col">PD</th>
              </tr>
            </thead>
            <tbody>
              ${buildEyeRow('OD', 'Right Eye', rightEye)}
              ${buildEyeRow('OS', 'Left Eye', leftEye)}
            </tbody>
          </table>
        </div>
      </article>
    `;
  }

  /**
   * Build the pagination controls.
   * @param {number} page
   * @param {number} pageCount
   * @param {number} total
   * @returns {string}
   */
  function buildPaginationHTML(page, pageCount, total) {
    if (pageCount <= 1) return '';
    return `
      <nav class="prescription-list__pagination" aria-label="Prescription pages">
        <button
          type="button"
          class="prescription-list__page-btn"
          data-page-action="prev"
          aria-label="Previous page"
          ${page <= 1 ? 'disabled' : ''}
        >Previous</button>
        <span class="prescription-list__page-status" aria-live="polite">
          Page ${page} of ${pageCount}
          <span class="prescription-list__page-total">(${total} total)</span>
        </span>
        <button
          type="button"
          class="prescription-list__page-btn"
          data-page-action="next"
          aria-label="Next page"
          ${page >= pageCount ? 'disabled' : ''}
        >Next</button>
      </nav>
    `;
  }

  /**
   * Build skeleton loading HTML.
   * @returns {string}
   */
  function buildSkeletonHTML() {
    return `
      <div class="prescription-list__skeleton" role="status">
        <span class="prescription-list__sr-only">Loading prescriptions...</span>
        <div class="prescription-list__skeleton-line prescription-list__skeleton-line--title"></div>
        <div class="prescription-list__skeleton-line"></div>
        <div class="prescription-list__skeleton-line prescription-list__skeleton-line--short"></div>
        <div class="prescription-list__skeleton-line"></div>
      </div>
    `;
  }

  /**
   * Build error state HTML.
   * @param {string} message
   * @returns {string}
   */
  function buildErrorHTML(message) {
    return `
      <div class="prescription-list__error" role="alert">
        <span class="prescription-list__error-icon" aria-hidden="true">!</span>
        <span class="prescription-list__error-message">${escHtml(message)}</span>
      </div>
    `;
  }

  /**
   * Build the complete inner HTML for the component.
   * @param {Object} state
   * @returns {string}
   */
  function buildHTML(state) {
    const { title, rows, page, pageCount, loading, error, total } = state;
    const showCount = !loading && !error && total > 0;
    const count = showCount
      ? `<span class="prescription-list__count">${total} prescription${total === 1 ? '' : 's'}</span>`
      : '';

    let body;
    if (loading) {
      body = buildSkeletonHTML();
    } else if (error) {
      body = buildErrorHTML(error);
    } else if (total === 0) {
      body = '<div class="prescription-list__empty">No prescriptions recorded.</div>';
    } else {
      body = rows.map((rx, index) => buildItemHTML(rx, index + 1)).join('');
      body += buildPaginationHTML(page, pageCount, total);
    }

    return `
      <div class="prescription-list__header">
        <h2 class="prescription-list__title">${escHtml(title)}</h2>
        ${count}
      </div>
      <div class="prescription-list__body">
        ${body}
      </div>
    `;
  }

  /* ------------------------------------------------------------------
     PrescriptionList Class
     ------------------------------------------------------------------ */

  /**
   * @class PrescriptionList
   * @param {HTMLElement} mountEl  Container element to render into
   * @param {Object}      [opts]   Configuration options
   */
  function PrescriptionList(mountEl, opts) {
    if (!(this instanceof PrescriptionList)) {
      return new PrescriptionList(mountEl, opts);
    }

    if (typeof HTMLElement === 'undefined' || !(mountEl instanceof HTMLElement)) {
      throw new TypeError('PrescriptionList: mountEl must be an HTMLElement');
    }

    this._mount = mountEl;
    this._cfg = this._resolveConfig(opts || {});
    this._all = parseArray(this._cfg.prescriptions);
    this._page = 1;
    this._loading = false;
    this._error = '';
    this._root = null;

    this._createRoot();
    this._render();
  }

  /**
   * Resolve configuration from options and data-* attributes.
   * Explicit options take precedence over data attributes.
   * @private
   * @param {Object} opts
   * @returns {Object}
   */
  PrescriptionList.prototype._resolveConfig = function (opts) {
    const el = this._mount;

    const fromData = (name, fallback) => {
      const raw = el.dataset ? el.dataset[name] : undefined;
      if (raw !== undefined && raw !== '') return raw;
      if (opts[name] !== undefined) return opts[name];
      return fallback;
    };

    const rawSize = fromData('pageSize', DEFAULT_PAGE_SIZE);
    const pageSize = Math.max(1, parseInt(rawSize, 10) || DEFAULT_PAGE_SIZE);

    let prescriptions = opts.prescriptions;
    if (prescriptions === undefined && el.dataset) {
      prescriptions = el.dataset.prescriptions;
    }

    return {
      title: fromData('title', DEFAULT_TITLE),
      pageSize,
      prescriptions,
      fetchPrescriptions:
        typeof opts.fetchPrescriptions === 'function' ? opts.fetchPrescriptions : null,
    };
  };

  /**
   * Create the component wrapper once and attach delegated events.
   * @private
   */
  PrescriptionList.prototype._createRoot = function () {
    let root = this._mount.querySelector('.prescription-list-component');

    if (!root) {
      root = document.createElement('div');
      root.className = 'prescription-list-component';
      this._mount.appendChild(root);
    }

    root.setAttribute('role', 'region');
    root.setAttribute('aria-label', `${this._cfg.title} list`);

    root.addEventListener('click', (event) => {
      const target = event.target;
      if (!target || typeof target.closest !== 'function') return;
      const button = target.closest('[data-page-action]');
      if (!button || button.disabled) return;

      const action = button.getAttribute('data-page-action');
      if (action === 'prev') {
        this.prevPage();
      } else if (action === 'next') {
        this.nextPage();
      }
    });

    this._root = root;
  };

  /**
   * Number of pages for the current data set.
   * @returns {number}
   */
  PrescriptionList.prototype.getPageCount = function () {
    return Math.max(1, Math.ceil(this._all.length / this._cfg.pageSize));
  };

  /**
   * Render the component into the mount element.
   * @private
   */
  PrescriptionList.prototype._render = function () {
    if (!this._root) return;

    const pageCount = this.getPageCount();
    if (this._page > pageCount) this._page = pageCount;
    if (this._page < 1) this._page = 1;

    const start = (this._page - 1) * this._cfg.pageSize;
    const rows = this._all.slice(start, start + this._cfg.pageSize);

    if (this._loading) {
      this._root.classList.add('is-loading');
    } else {
      this._root.classList.remove('is-loading');
    }
    this._root.setAttribute('aria-busy', this._loading ? 'true' : 'false');

    this._root.innerHTML = buildHTML({
      title: this._cfg.title,
      rows,
      page: this._page,
      pageCount,
      loading: this._loading,
      error: this._error,
      total: this._all.length,
    });
  };

  /**
   * Show a loading skeleton state.
   */
  PrescriptionList.prototype.showLoading = function () {
    this._loading = true;
    this._error = '';
    this._render();
  };

  /**
   * Show an error state with an optional message.
   * @param {string} [message]
   */
  PrescriptionList.prototype.showError = function (message) {
    this._loading = false;
    this._error = message || 'Could not load prescriptions.';
    this._render();
  };

  /**
   * Replace the prescription data set and reset to the first page.
   * @param {Array|string} prescriptions
   */
  PrescriptionList.prototype.setPrescriptions = function (prescriptions) {
    this._all = parseArray(prescriptions);
    this._page = 1;
    this._loading = false;
    this._error = '';
    this._render();
  };

  /**
   * Fetch prescriptions through the injected provider.
   * @returns {Promise<Array>}
   */
  PrescriptionList.prototype.load = function () {
    const provider = this._cfg.fetchPrescriptions;
    if (typeof provider !== 'function') return Promise.resolve(this._all);

    this.showLoading();

    return Promise.resolve()
      .then(() => provider())
      .then((rows) => {
        this.setPrescriptions(rows);
        return this._all;
      })
      .catch((err) => {
        const message = err && err.message ? err.message : 'Could not load prescriptions.';
        this.showError(message);
        return [];
      });
  };

  /**
   * Navigate to a specific page (clamped to valid range).
   * @param {number} page
   */
  PrescriptionList.prototype.goToPage = function (page) {
    const next = Math.min(Math.max(1, parseInt(page, 10) || 1), this.getPageCount());
    if (next === this._page) return;
    this._page = next;
    this._render();
  };

  /**
   * Advance one page.
   */
  PrescriptionList.prototype.nextPage = function () {
    this.goToPage(this._page + 1);
  };

  /**
   * Go back one page.
   */
  PrescriptionList.prototype.prevPage = function () {
    this.goToPage(this._page - 1);
  };

  /**
   * Update configuration and re-render.
   * @param {Object} newOpts
   */
  PrescriptionList.prototype.update = function (newOpts) {
    if (!this._root) return;
    const opts = newOpts || {};

    if (opts.title !== undefined) {
      this._cfg.title = opts.title;
    }
    if (opts.pageSize !== undefined) {
      this._cfg.pageSize = Math.max(1, parseInt(opts.pageSize, 10) || this._cfg.pageSize);
    }
    if (opts.fetchPrescriptions !== undefined) {
      this._cfg.fetchPrescriptions =
        typeof opts.fetchPrescriptions === 'function' ? opts.fetchPrescriptions : null;
    }
    if (opts.prescriptions !== undefined) {
      this._all = parseArray(opts.prescriptions);
      this._page = 1;
    }

    this._loading = false;
    this._error = '';
    this._root.setAttribute('aria-label', `${this._cfg.title} list`);
    this._render();
  };

  /**
   * Unmount the component and remove its DOM node.
   */
  PrescriptionList.prototype.destroy = function () {
    if (this._root && this._root.parentNode) {
      this._root.parentNode.removeChild(this._root);
    }
    this._root = null;
    this._cfg = null;
    this._all = [];
    this._mount = null;
  };

  return PrescriptionList;
});
