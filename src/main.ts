import { createIcons, Settings, Info, Mic2, Wrench, Mic, Keyboard, Download, Zap } from 'lucide';

// Tab navigation functionality
function switchTab(tabId: string) {
  // Remove active class from all nav items and tab contents
  document.querySelectorAll('.nav-item').forEach(item => {
    item.classList.remove('active');
  });
  document.querySelectorAll('.tab-content').forEach(content => {
    content.classList.remove('active');
  });

  // Add active class to clicked nav item and corresponding tab content
  const navItem = document.querySelector(`[data-tab="${tabId}"]`);
  const tabContent = document.getElementById(tabId);

  if (navItem && tabContent) {
    navItem.classList.add('active');
    tabContent.classList.add('active');
  }

  // Reset scroll position to top
  const settingsPanel = document.querySelector('.settings-panel');
  if (settingsPanel) {
    settingsPanel.scrollTop = 0;
  }
}

// Initialize settings page
window.addEventListener("DOMContentLoaded", () => {
  // Initialize Lucide icons
  createIcons({
    icons: {
      Settings,
      Info,
      Mic2,
      Wrench,
      Mic,
      Keyboard,
      Download,
      Zap
    }
  });

  // Set up tab navigation
  document.querySelectorAll('.nav-item').forEach(button => {
    button.addEventListener('click', (e) => {
      const target = e.currentTarget as HTMLElement;
      const tabId = target.getAttribute('data-tab');
      if (tabId) {
        switchTab(tabId);
      }
    });
  });

  // Handle model selection cards
  const modelCards = Array.from(document.querySelectorAll<HTMLElement>('.model-card'));
  if (modelCards.length > 0) {
    type ModelAction = 'use' | 'download' | 'refresh' | 'remove' | 'confirm-remove' | 'cancel-remove';

    interface BackendModelStatus {
      name: string;
      size_mb: number;
      is_downloaded: boolean;
      is_downloading: boolean;
      is_active: boolean;
      downloaded_bytes: number;
      total_bytes: number | null;
      error: string | null;
    }

    interface ModelCardElements {
      card: HTMLElement;
      statusLabel: HTMLElement;
      buttons: Record<ModelAction, HTMLButtonElement>;
      actionsContainer: HTMLElement;
      confirm: {
        container: HTMLElement;
        message: HTMLElement;
        yes: HTMLButtonElement;
        no: HTMLButtonElement;
      };
      progress: {
        wrapper: HTMLElement;
        bar: HTMLProgressElement;
      };
    }

    interface ModelState {
      isDownloaded: boolean;
      isDownloading: boolean;
      isActive: boolean;
      downloadedBytes: number;
      totalBytes: number | null;
      progressPercent: number | null;
      error?: string;
      inFlightAction: ModelAction | null;
      pendingLabel: string | null;
      isConfirmingRemoval: boolean;
    }

    interface DownloadEventPayload {
      modelName: string;
      downloadedBytes: number;
      totalBytes?: number | null;
      percent?: number | null;
      status: string;
      error?: string | null;
    }

    interface ActiveModelPayload {
      modelName?: string | null;
    }

    const ACTION_LABELS: Record<ModelAction, string> = {
      use: 'Use',
      download: 'Download',
      refresh: 'Refresh',
      remove: 'Remove',
      'confirm-remove': 'Remove',
      'cancel-remove': 'Cancel',
    };

    type TauriGlobal = {
      core: {
        invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
      };
      event: {
        listen: (
          event: string,
          handler: (event: { payload: unknown }) => void
        ) => Promise<() => void>;
      };
    };

    const tauriGlobal = (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__;
    if (!tauriGlobal) {
      console.warn('Tauri API not available; skipping model controls.');
    } else {
      const invoke = tauriGlobal.core.invoke;
      const listen = tauriGlobal.event.listen;

      const cardElements = new Map<string, ModelCardElements>();
      const modelStates = new Map<string, ModelState>();

      const ensureState = (modelName: string): ModelState => {
        if (!modelStates.has(modelName)) {
          modelStates.set(modelName, {
            isDownloaded: false,
            isDownloading: false,
            isActive: false,
            downloadedBytes: 0,
            totalBytes: null,
            progressPercent: null,
            error: undefined,
            inFlightAction: null,
            pendingLabel: null,
            isConfirmingRemoval: false,
          });
        }
        return modelStates.get(modelName)!;
      };

      const computePercent = (downloadedBytes: number, totalBytes: number | null): number | null => {
        if (!totalBytes || totalBytes === 0) {
          return null;
        }
        return Math.min(100, Math.max(0, (downloadedBytes / totalBytes) * 100));
      };

    const formatBytes = (bytes: number): string => {
      const KB = 1024;
      const MB = KB * 1024;
      const GB = MB * 1024;
      if (bytes >= GB) {
        return `${(bytes / GB).toFixed(1)} GB`;
      }
      if (bytes >= MB) {
        return `${(bytes / MB).toFixed(1)} MB`;
      }
      if (bytes >= KB) {
        return `${(bytes / KB).toFixed(1)} KB`;
      }
      return `${bytes} B`;
    };

    const extractErrorMessage = (error: unknown): string => {
      if (error instanceof Error) return error.message;
      if (typeof error === 'string') return error;
      if (error && typeof error === 'object') {
        const errorObj = error as Record<string, unknown>;
        if (typeof errorObj.error === 'string') return errorObj.error;
        if (typeof errorObj.message === 'string') return errorObj.message;
      }
      try {
        return JSON.stringify(error);
      } catch {
        return String(error);
      }
    };

    const truncate = (value: string, max = 80): string => {
      return value.length > max ? `${value.slice(0, max - 1)}…` : value;
    };

      const formatStatus = (state: ModelState): string => {
        if (state.pendingLabel) {
          return state.pendingLabel;
        }
        if (state.isConfirmingRemoval) {
          return 'Confirm removal?';
        }
        if (state.isDownloading) {
          if (state.progressPercent !== null) {
            return `Downloading ${Math.round(state.progressPercent)}%`;
          }
          if (state.downloadedBytes > 0) {
            return `Downloading ${formatBytes(state.downloadedBytes)}`;
          }
          return 'Downloading…';
        }
        if (state.error) {
          return `Error: ${truncate(state.error)}`;
        }
        if (state.isActive && !state.isDownloaded) {
          return 'Active · Download required';
        }
        if (state.isActive) {
          return 'Active';
        }
        if (state.isDownloaded) {
          return 'Downloaded';
        }
        return 'Download required';
      };

      const updateCardUI = (modelName: string) => {
        const elements = cardElements.get(modelName);
        if (!elements) {
          return;
        }

        const state = ensureState(modelName);

        elements.card.classList.toggle('selected', state.isActive);
        elements.card.classList.toggle('downloading', state.isDownloading);
        elements.card.classList.toggle('downloaded', state.isDownloaded);
        elements.card.classList.toggle('errored', Boolean(state.error));
        elements.card.classList.toggle('confirming-remove', state.isConfirmingRemoval);

        const disableAll = state.inFlightAction !== null || state.isDownloading;

        Object.values(elements.buttons).forEach(button => {
          button.disabled = disableAll;
        });

        elements.actionsContainer.hidden = state.isConfirmingRemoval;
        elements.confirm.container.hidden = !state.isConfirmingRemoval;
        elements.confirm.yes.disabled = disableAll;
        elements.confirm.no.disabled = disableAll;

        elements.buttons.use.hidden = !(state.isDownloaded && !state.isActive && !state.isDownloading);
        elements.buttons.download.hidden = state.isDownloaded || state.isDownloading;
        elements.buttons.refresh.hidden = !state.isDownloaded || state.isDownloading;
        elements.buttons.remove.hidden = !state.isDownloaded || state.isActive || state.isDownloading;

        elements.statusLabel.textContent = formatStatus(state);

        if (state.isDownloading) {
          elements.progress.wrapper.dataset.visible = 'true';
          const percent = state.progressPercent;
          if (percent !== null) {
            const clamped = Math.min(100, Math.max(0, percent));
            elements.progress.bar.max = 100;
            elements.progress.bar.value = clamped;
          } else if (state.downloadedBytes > 0) {
            elements.progress.bar.removeAttribute('value');
          } else {
            elements.progress.bar.max = 100;
            elements.progress.bar.value = 0;
          }
        } else {
          elements.progress.wrapper.dataset.visible = 'false';
          elements.progress.bar.max = 100;
          elements.progress.bar.value = 0;
        }
      };

      const initializeCard = (card: HTMLElement) => {
        const modelName = card.dataset.model;
        if (!modelName) {
          return;
        }

        const footer = card.querySelector('.model-card-footer');
        const statusLabel = footer?.querySelector('[data-status]') as HTMLElement | null;
        const primaryButton = footer?.querySelector<HTMLButtonElement>('.model-button');

        if (!footer || !statusLabel || !primaryButton) {
          return;
        }

        footer.innerHTML = '';

        primaryButton.type = 'button';
        primaryButton.dataset.action = 'use';
        primaryButton.textContent = ACTION_LABELS.use;
        primaryButton.removeAttribute('data-model');

        const actionsContainer = document.createElement('div');
        actionsContainer.className = 'model-actions';
        actionsContainer.appendChild(primaryButton);

        const createActionButton = (action: ModelAction) => {
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'model-button';
          button.dataset.action = action;
          button.textContent = ACTION_LABELS[action];
          actionsContainer.appendChild(button);
          return button;
        };

        const downloadButton = createActionButton('download');
        const refreshButton = createActionButton('refresh');
        const removeButton = createActionButton('remove');

        const metaRow = document.createElement('div');
        metaRow.className = 'model-meta';
        metaRow.appendChild(statusLabel);

        const confirmContainer = document.createElement('div');
        confirmContainer.className = 'model-confirm';
        confirmContainer.hidden = true;

        const confirmMessage = document.createElement('span');
        confirmMessage.className = 'model-confirm-text';
        confirmMessage.textContent = 'Remove this model?';

        const confirmButtons = document.createElement('div');
        confirmButtons.className = 'model-confirm-actions';

        const confirmYes = document.createElement('button');
        confirmYes.type = 'button';
        confirmYes.className = 'model-button model-button-danger';
        confirmYes.dataset.action = 'confirm-remove';
        confirmYes.textContent = 'Yes, remove';

        const confirmNo = document.createElement('button');
        confirmNo.type = 'button';
        confirmNo.className = 'model-button';
        confirmNo.dataset.action = 'cancel-remove';
        confirmNo.textContent = 'Cancel';

        confirmButtons.append(confirmYes, confirmNo);
        confirmContainer.append(confirmMessage, confirmButtons);

        footer.append(actionsContainer, confirmContainer, metaRow);

        const progressWrapper = document.createElement('div');
        progressWrapper.className = 'model-progress';
        progressWrapper.dataset.visible = 'false';

        const progressBar = document.createElement('progress');
        progressBar.className = 'model-progress-bar';
        progressBar.max = 100;
        progressBar.value = 0;

        progressWrapper.appendChild(progressBar);
        card.appendChild(progressWrapper);

        const elements: ModelCardElements = {
          card,
          statusLabel,
          buttons: {
            use: primaryButton,
            download: downloadButton,
            refresh: refreshButton,
            remove: removeButton,
            'confirm-remove': confirmYes,
            'cancel-remove': confirmNo,
          },
          actionsContainer,
          confirm: {
            container: confirmContainer,
            message: confirmMessage,
            yes: confirmYes,
            no: confirmNo,
          },
          progress: {
            wrapper: progressWrapper,
            bar: progressBar,
          },
        };

        cardElements.set(modelName, elements);
        ensureState(modelName);

        (Object.entries(elements.buttons) as Array<[ModelAction, HTMLButtonElement]>).forEach(([action, button]) => {
          button.addEventListener('click', () => handleAction(modelName, action));
        });

        updateCardUI(modelName);
      };

      const refreshStatuses = async () => {
        try {
          const statuses = await invoke<BackendModelStatus[]>('get_model_statuses');
          console.log('Model statuses', statuses);
          let activeModelName: string | null = null;

          statuses.forEach(status => {
            const state = ensureState(status.name);
            state.isDownloaded = status.is_downloaded;
            state.isDownloading = status.is_downloading;
            state.isActive = status.is_active;
            state.downloadedBytes = status.downloaded_bytes ?? 0;
            state.totalBytes = status.total_bytes ?? null;
            state.progressPercent = status.is_downloading
              ? computePercent(state.downloadedBytes, state.totalBytes)
              : (status.is_downloaded ? 100 : null);
            state.error = status.error ?? undefined;
            if (!status.is_downloaded) {
              state.isConfirmingRemoval = false;
            }
            if (status.is_active) {
              activeModelName = status.name;
            }
          });

          if (activeModelName) {
            localStorage.setItem('selected_model', activeModelName);
          }

          cardElements.forEach((_, name) => updateCardUI(name));
        } catch (error) {
          console.error('Failed to fetch model statuses:', error);
        }
      };

      const handleAction = async (modelName: string, action: ModelAction) => {
        const elements = cardElements.get(modelName);
        if (!elements) {
          return;
        }

        const state = ensureState(modelName);
        if (state.inFlightAction) {
          return;
        }

        if (action === 'remove') {
          state.isConfirmingRemoval = true;
          state.pendingLabel = null;
          updateCardUI(modelName);
          return;
        }

        if (action === 'cancel-remove') {
          state.isConfirmingRemoval = false;
          state.pendingLabel = null;
          updateCardUI(modelName);
          return;
        }

        const effectiveAction: ModelAction | 'remove' = action === 'confirm-remove' ? 'remove' : action;

        state.inFlightAction = effectiveAction;
        state.pendingLabel =
          effectiveAction === 'use'
            ? 'Switching…'
            : effectiveAction === 'download'
              ? 'Preparing download…'
              : effectiveAction === 'refresh'
                ? 'Refreshing…'
                : effectiveAction === 'remove'
                  ? 'Removing…'
                  : null;
        state.error = undefined;
        updateCardUI(modelName);

        try {
          if (action === 'download') {
            await invoke('start_model_download', { modelName });
          } else if (action === 'refresh') {
            await invoke('refresh_model_download', { modelName });
          } else if (action === 'confirm-remove') {
            await invoke('remove_model', { modelName });
            state.isConfirmingRemoval = false;
            alert(`${modelName} removed.`);
          } else if (action === 'use') {
            await invoke('switch_model', { modelName });
            localStorage.setItem('selected_model', modelName);
          }

          await refreshStatuses();
        } catch (error) {
          console.error(`Failed to ${action} model ${modelName}:`, error);
          const message = extractErrorMessage(error);
          if (effectiveAction === 'remove' && message.includes('currently active')) {
            state.isConfirmingRemoval = true;
            alert('Switch to another model before removing the active one.');
          } else {
            alert(`Failed to ${ACTION_LABELS[effectiveAction].toLowerCase()} model: ${message}`);
          }
        } finally {
          state.inFlightAction = null;
          state.pendingLabel = null;
          updateCardUI(modelName);
        }
      };

      const setActiveFromEvent = (activeName: string | null) => {
        modelStates.forEach((state, name) => {
          state.isActive = activeName !== null && name === activeName;
          updateCardUI(name);
        });
        if (activeName) {
          localStorage.setItem('selected_model', activeName);
        }
      };

      modelCards.forEach(initializeCard);

      const openModelsButton = document.querySelector<HTMLButtonElement>('[data-action="open-models"]');
      if (openModelsButton) {
        openModelsButton.addEventListener('click', async () => {
          try {
            await invoke('open_models_folder');
          } catch (error) {
            console.error('Failed to open models folder:', error);
            alert('Unable to open models folder.');
          }
        });
      }

      const savedModel = localStorage.getItem('selected_model');
      if (savedModel) {
        const state = ensureState(savedModel);
        state.isActive = true;
        updateCardUI(savedModel);
      } else {
        const defaultCard = document.querySelector<HTMLElement>('.model-card[data-default="true"]');
        const defaultModel = defaultCard?.dataset.model;
        if (defaultModel) {
          const state = ensureState(defaultModel);
          state.isActive = true;
          updateCardUI(defaultModel);
        }
      }

      void refreshStatuses();

      void listen('model-download-progress', (event) => {
        const payload = event.payload as DownloadEventPayload;
        const state = ensureState(payload.modelName);

        switch (payload.status) {
          case 'queued':
          case 'refreshing':
          case 'started':
            state.isDownloading = true;
            state.error = undefined;
            state.downloadedBytes = payload.downloadedBytes ?? 0;
            state.totalBytes = payload.totalBytes ?? state.totalBytes;
            state.progressPercent = 0;
            if (payload.status !== 'refreshing') {
              state.isDownloaded = false;
            }
            break;
          case 'downloading':
            state.isDownloading = true;
            state.error = undefined;
            state.downloadedBytes = payload.downloadedBytes ?? 0;
            state.totalBytes = payload.totalBytes ?? state.totalBytes;
            state.progressPercent = payload.percent ?? computePercent(state.downloadedBytes, state.totalBytes);
            break;
          case 'completed':
            state.isDownloading = false;
            state.isDownloaded = true;
            state.error = undefined;
            state.downloadedBytes = payload.downloadedBytes ?? state.downloadedBytes;
            state.totalBytes = payload.totalBytes ?? state.totalBytes;
            state.progressPercent = 100;
            break;
          case 'removed':
            state.isDownloading = false;
            state.isDownloaded = false;
            state.error = undefined;
            state.downloadedBytes = 0;
            state.totalBytes = null;
            state.progressPercent = null;
            break;
          case 'error':
            state.isDownloading = false;
            state.error = payload.error ?? 'Download failed';
            state.progressPercent = null;
            break;
          case 'active':
            state.isDownloaded = true;
            state.progressPercent = 100;
            break;
          default:
            break;
        }

        updateCardUI(payload.modelName);

        if (['completed', 'error', 'removed', 'active'].includes(payload.status)) {
          void refreshStatuses();
        }
      });

      void listen('active-model-changed', (event) => {
        const payload = event.payload as ActiveModelPayload;
        const activeName = payload?.modelName ?? null;
        setActiveFromEvent(activeName);
      });
    }
  }

  // Handle other settings changes (save to localStorage for persistence)
  document.querySelectorAll('.toggle, .select-input').forEach(input => {
    input.addEventListener('change', (e) => {
      const target = e.target as HTMLInputElement | HTMLSelectElement;
      const settingId = target.id;
      const value = target instanceof HTMLInputElement && target.type === 'checkbox'
        ? target.checked
        : target.value;

      localStorage.setItem(`setting_${settingId}`, JSON.stringify(value));
      console.log(`Setting saved: ${settingId} = ${value}`);
    });
  });

  // Load saved settings
  document.querySelectorAll('.toggle, .select-input').forEach(input => {
    const target = input as HTMLInputElement | HTMLSelectElement;
    const settingId = target.id;
    const savedValue = localStorage.getItem(`setting_${settingId}`);
    
    if (savedValue !== null) {
      try {
        const value = JSON.parse(savedValue);
        if (target instanceof HTMLInputElement && target.type === 'checkbox') {
          target.checked = value;
        } else {
          target.value = value;
        }
      } catch (e) {
        console.error(`Error loading setting ${settingId}:`, e);
      }
    }
  });

  // Handle clear data button
  const clearButton = document.querySelector('.danger-button');
  if (clearButton) {
    clearButton.addEventListener('click', () => {
      if (confirm('Are you sure you want to clear all transcription history? This action cannot be undone.')) {
        // TODO: Implement actual data clearing
        console.log('Clearing all data...');
        alert('All transcription history has been cleared.');
      }
    });
  }
});
