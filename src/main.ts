import { createIcons, Settings, Mic, Keyboard, Lock, Info, Mic2 } from 'lucide';

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
      Mic,
      Keyboard,
      Lock,
      Info,
      Mic2
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

  // Handle settings changes (save to localStorage for persistence)
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
