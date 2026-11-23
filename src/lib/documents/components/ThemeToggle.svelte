<script lang="ts">
  // Switches Tessera's theme (one setting for the whole app).
  import { app } from '../../stores/app.svelte';
  import Icon from './Icon.svelte';

  const dark = $derived(document.documentElement.dataset.theme !== 'light' && app.settings.theme !== 'light');

  function toggle() {
    void app.saveSettings({ theme: dark ? 'light' : 'dark' });
  }
</script>

<button class="icon-btn" title="Switch to {dark ? 'light' : 'dark'} theme" onclick={toggle}>
  <Icon name={dark ? 'sun' : 'moon'} />
</button>
