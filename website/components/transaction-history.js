/* ==========================================================================
   RetinaX — Transaction History Component (transaction-history.js)
   Displays recent contract invocations for the connected wallet.

   Fetches from Soroban RPC invocation history or accepts entries prop.
   Renders with audit-log-table patterns.

   The component does not fetch data itself by default. Set the `entries` property
   to render them, or it will attempt to use window.SorobanRPC.getHistory().

   Entry shape (from SorobanRPC.invocationHistory):
     { action, microContract, contractAddress, status, timestamp, error }
   ========================================================================== */

(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.TransactionHistory = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  const ElementBase = typeof HTMLElement !== 'undefined' ? HTMLElement : function () {};
  const DEFAULT_PAGE_SIZE = 10;

  // Ledger timestamps are in seconds. Larger values are treated as milliseconds.
  const MILLISECONDS_THRESHOLD = 1e12;
  const DAY_SECONDS = 86400;

  function toSeconds(timestamp) {
    const value = Number(timestamp);
    if (!Number.isFinite(value) || value < 0) return null;
    return value >= MILLISECONDS_THRESHOLD ? value / 1000 : value;
  }

  // Turns a YYYY-MM-DD value from a date input into the first and last second of that UTC day.
  function dayBounds(value) {
    if (!/^\\d{4}-\\d{2}-\\d{2}$/.test(String(value || ''))) return null;
    const start = Date.parse(`${value}T00:00:00Z`) / 1000;
    if (Number.isNaN(start)) return null;
    return { start, end: start + DAY_SECONDS - 1 };
  }

  /** Formats a ledger timestamp as "YYYY-MM-DD HH:MM:SS UTC". Returns "-" for invalid input. */
  function formatTimestamp(timestamp) {
    const seconds = toSeconds(timestamp);
    if (seconds === null) return '-';
    const date = new Date(seconds * 1000);
    if (Number.isNaN(date.getTime())) return '-';
    return `${date.toISOString().slice(0, 19).replace('T', ' ')} UTC`;
  }

  /** Filters entries by a free-text search on action or actor, and by an inclusive UTC date range. */
  function filterEntries(entries, options) {
    const { search = '', from = '', to = '' } = options || {};
    const query = String(search).trim().toLowerCase();
    const fromBounds = dayBounds(from);
    const toBounds = dayBounds(to);
    const list = Array.isArray(entries) ? entries.filter(Boolean) : [];

    return list
      .filter((entry) => {
        const seconds = toSeconds(entry.timestamp);
        if (fromBounds && (seconds === null || seconds < fromBounds.start)) return false;
        if (toBounds && (seconds === null || seconds > toBounds.end)) return false;
        if (!query) return true;
        const fields = [entry.action, entry.microContract, entry.contractAddress];
        return fields.some((field) =>
          String(field ?? '')
            .toLowerCase()
            .includes(query)
        );
      })
      .sort((a, b) => (Number(b.timestamp) || 0) - (Number(a.timestamp) || 0));
  }

  /** Returns one page of items. */
  function paginate(items, page, pageSize) {
    const size = Math.max(1, Math.floor(pageSize) || DEFAULT_PAGE_SIZE);
    const list = Array.isArray(items) ? items : [];
    const pageCount = Math.max(1, Math.ceil(list.length / size));
    const current = Math.min(Math.max(1, Math.floor(page) || 1), pageCount);
    const start = (current - 1) * size;
    return {
      items: list.slice(start, start + size),
      page: current,
      pageCount,
      total: list.length,
    };
  }

  /** Maps a status to a human-readable label. */
  function statusLabel(status) {
    const map = { success: 'Confirmed', error: 'Failed', pending: 'Pending' };
    return map[String(status).toLowerCase()] || String(status);
  }

  /** Truncate middle of address/hash for UI display */
  function truncateMiddle(str, startChars = 6, endChars = 6) {
    if (!str || typeof str !== 'string') return '';
    if (str.length <= startChars + endChars) return str;
    return `${str.slice(0, startChars)}...${str.slice(-endChars)}`;
  }

  /** Gets a color class for the status. */
  function statusClass(status) {
    const map = { success: 'success', error: 'error', pending: 'pending' };
    return map[String(status).toLowerCase()] || 'default';
  }

  class TransactionHistory extends ElementBase {
    constructor() {
      super();
      this._entries = [];
      this._filters = { search: '', from: '', to: '' };
      this._page = 1;
      this._body = null;
      this._pageInfo = null;
      this._prev = null;
      this._next = null;
    }

    get entries() {
      return this._entries;
    }

    set entries(value) {
      this._entries = Array.isArray(value) ? value : [];
      this._page = 1;
      this.refresh();
    }

    connectedCallback() {
      if (!this._body) this.buildShell();
      this.refresh();
    }

    buildShell() {
      this.innerHTML =
        '<div class="transaction-history">' +
        '<form class="transaction-history__filters" role="search">' +
        '<label>Search action or contract' +
        '<input type="search" class="transaction-history__search" placeholder="e.g. grant_access"></label>' +
        '</label>' +
        '<label>From<input type="date" class="transaction-history__from"></label>' +
        '<label>To<input type="date" class="transaction-history__to"></label>' +
        '</form>' +
        '<p class="transaction-history__summary" aria-live="polite"></p>' +
        '<div class="transaction-history__scroll"><table>' +
        '<caption>Contract Invocations</caption>' +
        '<thead><tr>' +
        '<th scope="col">Timestamp</th><th scope="col">Action</th>' +
        '<th scope="col">Micro-Contract</th><th scope="col">Contract</th>' +
        '<th scope="col">Status</th>' +
        '</tr></thead>' +
        '<tbody class="transaction-history__body"></tbody>' +
        '</table></div>' +
        '<nav class="transaction-history__pager" aria-label="Transaction history pages">' +
        '<button type="button" class="transaction-history__prev">Previous</button>' +
        '<span class="transaction-history__page-info" aria-live="polite"></span>' +
        '<button type="button" class="transaction-history__next">Next</button>' +
        '</nav>' +
        '</div>';

      this._body = this.querySelector('.transaction-history__body');
      this._summary = this.querySelector('.transaction-history__summary');
      this._pageInfo = this.querySelector('.transaction-history__page-info');
      this._prev = this.querySelector('.transaction-history__prev');
      this._next = this.querySelector('.transaction-history__next');

      // Filters apply live, so Enter should not submit the form and reload the page.
      this.querySelector('.transaction-history__filters').addEventListener('submit', (event) => {
        event.preventDefault();
      });
      this.querySelector('.transaction-history__search').addEventListener('input', (event) => {
        this.setFilter('search', event.target.value);
      });
      this.querySelector('.transaction-history__from').addEventListener('input', (event) => {
        this.setFilter('from', event.target.value);
      });
      this.querySelector('.transaction-history__to').addEventListener('input', (event) => {
        this.setFilter('to', event.target.value);
      });
      this._prev.addEventListener('click', () => this.changePage(-1));
      this._next.addEventListener('click', () => this.changePage(1));
    }

    setFilter(name, value) {
      this._filters[name] = value;
      this._page = 1;
      this.refresh();
    }

    changePage(delta) {
      this._page += delta;
      this.refresh();
    }

    refresh() {
      if (!this._body) return;
      // If no entries prop set, try to fetch from SorobanRPC
      if (this._entries.length === 0 && typeof window !== 'undefined' && window.SorobanRPC) {
        this._entries = window.SorobanRPC.getHistory() || [];
      }
      const filtered = filterEntries(this._entries, this._filters);
      const view = paginate(filtered, this._page, DEFAULT_PAGE_SIZE);
      this._page = view.page;

      this._body.textContent = '';
      if (view.items.length === 0) {
        this._body.appendChild(this.buildEmptyRow());
      } else {
        view.items.forEach((entry) => this._body.appendChild(this.buildRow(entry)));
      }

      if (view.total === 0) {
        this._summary.textContent = 'No matching contract invocations found.';
      } else {
        this._summary.textContent = `${view.total} ${view.total === 1 ? 'invocation' : 'invocations'}`;
      }
      this._pageInfo.textContent = `Page ${view.page} of ${view.pageCount}`;
      this._prev.disabled = view.page <= 1;
      this._next.disabled = view.page >= view.pageCount;
    }

    buildRow(entry) {
      const row = document.createElement('tr');
      const cells = [
        ['Timestamp', formatTimestamp(entry.timestamp)],
        ['Action', entry.action || '-'],
        ['Micro-Contract', entry.microContract || '-'],
        ['Contract', entry.contractAddress ? truncateMiddle(entry.contractAddress) : '-'],
        [
          'Status',
          `<span class="status-${statusClass(entry.status)}">${statusLabel(entry.status)}</span>`,
        ],
      ];
      cells.forEach(([label, value]) => {
        const cell = document.createElement('td');
        cell.setAttribute('data-label', label);
        cell.innerHTML = value === undefined || value === null ? '' : String(value);
        row.appendChild(cell);
      });
      return row;
    }

    buildEmptyRow() {
      const row = document.createElement('tr');
      const cell = document.createElement('td');
      cell.colSpan = 5;
      cell.className = 'transaction-history__empty';
      cell.textContent = 'No contract invocations to show.';
      row.appendChild(cell);
      return row;
    }
  }

  if (typeof customElements !== 'undefined' && !customElements.get('transaction-history')) {
    customElements.define('transaction-history', TransactionHistory);
  }

  return {
    TransactionHistory,
    filterEntries,
    paginate,
    formatTimestamp,
    statusLabel,
    statusClass,
    DEFAULT_PAGE_SIZE,
    truncateMiddle,
  };
});
