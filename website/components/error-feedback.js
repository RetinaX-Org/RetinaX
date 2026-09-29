/* ==========================================================================
   RetinaX — Error Feedback Notification
   Toast-style error notifications for RPC call failures.
   Exposes window.showErrorFeedback(message) API.
   ========================================================================== */

(function () {
  'use strict';

  function showErrorFeedback(message, duration) {
    const container = getOrCreateContainer();

    const toast = document.createElement('div');
    toast.className = 'error-feedback-toast';
    toast.setAttribute('role', 'alert');
    toast.innerHTML = `
      <div class="error-feedback-content">
        <span class="error-feedback-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
        </span>
        <span class="error-feedback-message">${escapeHtml(message)}</span>
        <button class="error-feedback-close" aria-label="Dismiss error">&times;</button>
      </div>
    `;

    container.appendChild(toast);

    requestAnimationFrame(() => {
      toast.classList.add('visible');
    });

    const dismiss = () => {
      toast.classList.remove('visible');
      setTimeout(() => {
        if (toast.parentNode) {
          toast.parentNode.removeChild(toast);
        }
      }, 300);
    };

    toast.querySelector('.error-feedback-close').addEventListener('click', dismiss);

    const timeout = duration || 5000;
    setTimeout(dismiss, timeout);
  }

  function getOrCreateContainer() {
    let container = document.getElementById('error-feedback-container');
    if (!container) {
      container = document.createElement('div');
      container.id = 'error-feedback-container';
      container.className = 'error-feedback-container';
      document.body.appendChild(container);
    }
    return container;
  }

  function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
  }

  window.showErrorFeedback = showErrorFeedback;
})();
