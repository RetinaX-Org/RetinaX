/* ==========================================================================
   RetinaX — Global Loading Spinner
   Overlay spinner for contract read operations and async tasks.
   Exposes window.GlobalSpinner with show()/hide() API.
   ========================================================================== */

(function () {
  'use strict';

  class GlobalSpinner extends HTMLElement {
    static get observedAttributes() {
      return ['visible', 'message'];
    }

    constructor() {
      super();
      this._visible = false;
      this._message = 'Loading...';
      this._handleKeydown = this.handleKeydown.bind(this);
    }

    connectedCallback() {
      this.render();
      this.updateVisibility();
    }

    disconnectedCallback() {
      document.removeEventListener('keydown', this._handleKeydown);
    }

    attributeChangedCallback(name, oldValue, newValue) {
      if (oldValue === newValue) return;
      if (name === 'visible') {
        this._visible = newValue !== null && newValue !== 'false';
      } else if (name === 'message') {
        this._message = newValue || 'Loading...';
      }
      this.render();
      this.updateVisibility();
    }

    get visible() {
      return this._visible;
    }

    set visible(val) {
      this._visible = val;
      this.render();
      this.updateVisibility();
    }

    get message() {
      return this._message;
    }

    set message(val) {
      this._message = val || 'Loading...';
      this.render();
    }

    render() {
      this.innerHTML = `
        <div class="global-spinner-overlay" role="alert" aria-live="polite" aria-busy="${this._visible}">
          <div class="global-spinner-content">
            <div class="global-spinner-icon">
              <div class="global-spinner-ring"></div>
              <div class="global-spinner-ring"></div>
              <div class="global-spinner-ring"></div>
            </div>
            <p class="global-spinner-message">${this._message}</p>
          </div>
        </div>
      `;
    }

    updateVisibility() {
      const overlay = this.querySelector('.global-spinner-overlay');
      if (!overlay) return;

      if (this._visible) {
        overlay.classList.add('active');
        document.body.style.overflow = 'hidden';
        document.addEventListener('keydown', this._handleKeydown);
      } else {
        overlay.classList.remove('active');
        document.body.style.overflow = '';
        document.removeEventListener('keydown', this._handleKeydown);
      }
    }

    handleKeydown(e) {
      if (e.key === 'Escape' && this._visible) {
        this.hide();
      }
    }

    show(message) {
      if (message) this._message = message;
      this._visible = true;
      this.render();
      this.updateVisibility();
    }

    hide() {
      this._visible = false;
      this.render();
      this.updateVisibility();
    }
  }

  if (!customElements.get('global-spinner')) {
    customElements.define('global-spinner', GlobalSpinner);
  }

  window.GlobalSpinner = GlobalSpinner;
})();
