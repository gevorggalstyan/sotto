import { createIcons, Settings, Info, Mic2, AlertCircle, Mic, Keyboard, Download, Zap } from 'lucide';

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
}

// Initialize settings page
window.addEventListener("DOMContentLoaded", () => {
  // Initialize Lucide icons
  createIcons({
    icons: {
      Settings,
      Info,
      Mic2,
      AlertCircle,
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

  // Handle model selection changes
  const modelSelect = document.getElementById('model-selection') as HTMLSelectElement;
  if (modelSelect) {
    // Mark downloaded models with ✓
    async function updateModelIndicators() {
      try {
        // @ts-ignore - Tauri command
        const downloadedModels = await window.__TAURI__.core.invoke('get_downloaded_models') as string[];

        // Update all options
        Array.from(modelSelect.options).forEach(option => {
          const modelName = option.value;
          const isDownloaded = downloadedModels.includes(modelName);

          // Remove existing indicators
          option.text = option.text.replace(/^✓ /, '').replace(/^\[Download\] /, '');

          // Add appropriate indicator
          if (isDownloaded) {
            option.text = `✓ ${option.text}`;
          } else {
            option.text = `[Download] ${option.text}`;
          }
        });
      } catch (error) {
        console.error('Failed to get downloaded models:', error);
      }
    }

    // Update indicators on load
    updateModelIndicators();

    modelSelect.addEventListener('change', async (e) => {
      const target = e.target as HTMLSelectElement;
      const modelName = target.value;

      console.log(`Switching to model: ${modelName}`);

      try {
        // @ts-ignore - Tauri command
        const result = await window.__TAURI__.core.invoke('switch_model', { modelName });
        console.log(result);
        localStorage.setItem('selected_model', modelName);
        await updateModelIndicators(); // Refresh indicators after switch
        alert(`Successfully switched to ${modelName} model!`);
      } catch (error) {
        console.error('Failed to switch model:', error);
        alert(`Failed to switch model: ${error}`);
      }
    });

    // Load saved model selection
    const savedModel = localStorage.getItem('selected_model');
    if (savedModel) {
      modelSelect.value = savedModel;
    }
  }

  // Handle other settings changes (save to localStorage for persistence)
  document.querySelectorAll('.toggle, .select-input').forEach(input => {
    if (input.id === 'model-selection') return; // Skip model selection, handled above

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
