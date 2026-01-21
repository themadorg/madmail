const MadmailApp = (function() {
  'use strict';

  const state = {
    currentLink: '',
    registrationOpen: false,
    jitEnabled: false,
    selectedMethod: null
  };

  function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(() => {
        showNotification('کپی شد!', 'success');
      }).catch(err => {
        console.error('Failed to copy: ', err);
        fallbackCopyTextToClipboard(text);
      });
    } else {
      fallbackCopyTextToClipboard(text);
    }
  }

  function fallbackCopyTextToClipboard(text) {
    const textArea = document.createElement("textarea");
    textArea.value = text;
    textArea.style.position = "fixed";
    textArea.style.left = "-9999px";
    textArea.style.top = "0";
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();
    try {
      const successful = document.execCommand('copy');
      if (successful) {
        showNotification('کپی شد!', 'success');
      }
    } catch (err) {
      console.error('Fallback copy failed', err);
      showNotification('خطا در کپی کردن', 'danger');
    }
    document.body.removeChild(textArea);
  }

  function formatEmail(username, domain) {
    if (/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(domain)) {
      return `${username}@[${domain}]`;
    }
    return `${username}@${domain}`;
  }

  function generateRandomString(length) {
    const charset = "abcdefghijklmnopqrstuvwxyz0123456789";
    let result = "";
    for (let i = 0; i < length; i++) {
      result += charset.charAt(Math.floor(Math.random() * charset.length));
    }
    return result;
  }

  function showNotification(message, type = 'info') {
    const existingNotif = document.querySelector('.notification-toast');
    if (existingNotif) {
      existingNotif.remove();
    }

    const notification = document.createElement('div');
    notification.className = `notification-toast alert alert-${type}`;
    notification.textContent = message;
    notification.style.cssText = `
      position: fixed;
      top: 20px;
      right: 20px;
      z-index: 10000;
      min-width: 200px;
      max-width: 400px;
      padding: 16px 24px;
      border-radius: 12px;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
      animation: slideIn 0.3s ease-out;
    `;

    document.body.appendChild(notification);

    setTimeout(() => {
      notification.style.animation = 'fadeOut 0.3s ease-out';
      setTimeout(() => notification.remove(), 300);
    }, 3000);
  }

  const Modal = {
    activeModal: null,

    open: function(modalId) {
      const modal = document.getElementById(modalId);
      if (modal) {
        modal.classList.add('active');
        this.activeModal = modal;
        document.body.style.overflow = 'hidden';

        modal.addEventListener('click', (e) => {
          if (e.target === modal) {
            this.close(modalId);
          }
        });
      }
    },

    close: function(modalId) {
      const modal = document.getElementById(modalId);
      if (modal) {
        modal.classList.remove('active');
        this.activeModal = null;
        document.body.style.overflow = '';
      }
    },

    closeAll: function() {
      document.querySelectorAll('.modal.active').forEach(modal => {
        modal.classList.remove('active');
      });
      this.activeModal = null;
      document.body.style.overflow = '';
    }
  };

  const Accordion = {
    init: function() {
      document.querySelectorAll('.accordion-header').forEach(header => {
        header.addEventListener('click', function() {
          const item = this.closest('.accordion-item');
          const wasActive = item.classList.contains('active');

          const accordion = item.closest('.accordion');
          accordion.querySelectorAll('.accordion-item').forEach(i => {
            i.classList.remove('active');
          });

          if (!wasActive) {
            item.classList.add('active');
          }
        });
      });
    }
  };

  const RegistrationFlow = {
    config: {
      mailDomain: '',
      publicIP: '',
      mxDomain: '',
      turnOffTLS: false
    },

    init: function(config) {
      this.config = { ...this.config, ...config };
      state.registrationOpen = config.registrationOpen;
      state.jitEnabled = config.jitEnabled;
    },

    createDcloginLink: function(email, password) {
      const host = this.config.publicIP || this.config.mxDomain;
      const turnOffTLS = this.config.turnOffTLS;

      if (turnOffTLS) {
        return `dclogin:${email}/?p=${encodeURIComponent(password)}&v=1&ih=${host}&ip=143&sh=${host}&sp=25&is=plain&ss=plain&sc=3`;
      } else {
        return `dclogin:${email}/?p=${encodeURIComponent(password)}&v=1&ih=${host}&ip=993&sh=${host}&sp=465&ic=3&ss=default`;
      }
    },

    generateJIT: async function() {
      state.selectedMethod = 'jit';
      this.showLoading('در حال ایجاد اکانت فوری...');

      await this.simulateDelay(500);

      const username = generateRandomString(12);
      const password = generateRandomString(24);
      const email = formatEmail(username, this.config.mailDomain);

      state.currentLink = this.createDcloginLink(email, password);

      this.hideLoading();
      this.showResult(email, password);
    },

    generateStandard: async function() {
      state.selectedMethod = 'standard';
      this.showLoading('در حال ایجاد اکانت استاندارد...');
      this.hideError();

      try {
        const response = await fetch('/new', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json'
          }
        });

        if (!response.ok) {
          if (response.status === 403) {
            throw new Error('ثبت‌نام بسته شده است');
          }
          throw new Error('خطا در ایجاد اکانت: ' + response.status);
        }

        const data = await response.json();
        const email = data.email;
        const password = data.password;

        state.currentLink = this.createDcloginLink(email, password);

        this.hideLoading();
        this.showResult(email, password);
      } catch (error) {
        this.hideLoading();
        this.showError(error.message || 'خطا در ارتباط با سرور');
      }
    },

    showLoading: function(message) {
      const loadingEl = document.getElementById('loading-indicator');
      if (loadingEl) {
        loadingEl.textContent = message;
        loadingEl.classList.remove('hidden');
        loadingEl.style.display = 'block';
      }
    },

    hideLoading: function() {
      const loadingEl = document.getElementById('loading-indicator');
      if (loadingEl) {
        loadingEl.classList.add('hidden');
        loadingEl.style.display = 'none';
      }
    },

    showError: function(message) {
      const errorEl = document.getElementById('error-message');
      if (errorEl) {
        errorEl.textContent = message;
        errorEl.classList.remove('hidden');
        errorEl.style.display = 'block';
      }
      showNotification(message, 'danger');
    },

    hideError: function() {
      const errorEl = document.getElementById('error-message');
      if (errorEl) {
        errorEl.classList.add('hidden');
        errorEl.style.display = 'none';
      }
    },

    showResult: function(email, password) {
      const qrImg = document.getElementById('result-qr');
      if (qrImg) {
        qrImg.src = `/qr?data=${encodeURIComponent(state.currentLink)}`;
      }

      const qrContainer = document.getElementById('qr-container');
      if (qrContainer) {
        qrContainer.classList.remove('hidden');
        qrContainer.style.display = 'block';
      }

      const manualLink = document.getElementById('manual-link');
      if (manualLink) {
        manualLink.textContent = state.currentLink;
      }

      const accountResult = document.getElementById('account-result');
      if (accountResult) {
        accountResult.classList.remove('hidden');
        accountResult.style.display = 'block';
      }

      this.updateButtonStates();
      showNotification('اکانت با موفقیت ایجاد شد!', 'success');
    },

    updateButtonStates: function() {
      const hasLink = state.currentLink && state.currentLink.length > 0;

      const openBtn = document.getElementById('open-deltachat-btn');
      if (openBtn) {
        openBtn.disabled = !hasLink;
      }

      const copyBtn = document.getElementById('copy-link-btn');
      if (copyBtn) {
        copyBtn.disabled = !hasLink;
      }
    },

    openDeltaChat: function() {
      if (!state.currentLink) {
        this.showDeltaChatError();
        return;
      }

      this.hideDeltaChatError();

      let deltaChatOpened = false;
      let timeoutId = null;

      const cleanup = () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
        window.removeEventListener('blur', handleBlur);
        document.removeEventListener('visibilitychange', handleVisibilityChange);
      };

      const handleBlur = () => {
        deltaChatOpened = true;
        cleanup();
      };

      const handleVisibilityChange = () => {
        if (document.hidden) {
          deltaChatOpened = true;
          cleanup();
        }
      };

      window.addEventListener('blur', handleBlur, { once: true });
      document.addEventListener('visibilitychange', handleVisibilityChange, { once: true });

      try {
        window.location.href = state.currentLink;
      } catch (e) {
        const link = document.createElement('a');
        link.href = state.currentLink;
        link.style.display = 'none';
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
      }

      timeoutId = setTimeout(() => {
        cleanup();
        if (!deltaChatOpened) {
          this.showDeltaChatError();
        }
      }, 2500);
    },

    showDeltaChatError: function() {
      const errorEl = document.getElementById('deltachat-error');
      if (errorEl) {
        errorEl.classList.remove('hidden');
        errorEl.style.display = 'block';
      }
    },

    hideDeltaChatError: function() {
      const errorEl = document.getElementById('deltachat-error');
      if (errorEl) {
        errorEl.classList.add('hidden');
        errorEl.style.display = 'none';
      }
    },

    copyLink: function() {
      if (state.currentLink) {
        copyToClipboard(state.currentLink);
      }
    },

    simulateDelay: function(ms) {
      return new Promise(resolve => setTimeout(resolve, ms));
    }
  };

  const Animation = {
    observeElements: function() {
      const observer = new IntersectionObserver((entries) => {
        entries.forEach(entry => {
          if (entry.isIntersecting) {
            entry.target.style.animation = 'fadeInUp 0.6s ease-out forwards';
            observer.unobserve(entry.target);
          }
        });
      }, {
        threshold: 0.1
      });

      document.querySelectorAll('.feature-card, .registration-card, .step-item').forEach(el => {
        el.style.opacity = '0';
        observer.observe(el);
      });
    },

    init: function() {
      if ('IntersectionObserver' in window) {
        this.observeElements();
      }
    }
  };

  function init() {
    Accordion.init();
    Animation.init();

    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && Modal.activeModal) {
        Modal.closeAll();
      }
    });

    document.querySelectorAll('[data-copy]').forEach(el => {
      el.addEventListener('click', function() {
        const text = this.getAttribute('data-copy') || this.textContent;
        copyToClipboard(text);
      });
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  return {
    copyToClipboard,
    formatEmail,
    Modal,
    Accordion,
    RegistrationFlow,
    Animation,
    state,
    showNotification
  };
})();

window.copyToClipboard = MadmailApp.copyToClipboard;
window.formatEmail = MadmailApp.formatEmail;
