import './styles.css';

const root = document.querySelector('#app');

root.innerHTML = `
  <section class="skeleton-shell" aria-labelledby="app-title">
    <div class="eyebrow">Token Lens</div>
    <h1 id="app-title">v2 skeleton</h1>
    <p>
      Tauri 2 shell ready. The production renderer will be ported from Token Lens v1
      without redesigning its established UI and interaction model.
    </p>
  </section>
`;
