// QuectoClaw — Web UI HTML templates
//
// Inline HTML/HTMX templates served as Rust string constants.
// No external files or build steps required.

/// Main dashboard HTML page with HTMX auto-refresh.
pub const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>QuectoClaw Dashboard</title>
  <script src="https://unpkg.com/htmx.org@2.0.4"></script>
  <style>
    :root {
      --bg: #0f1117;
      --surface: #1a1d27;
      --surface2: #252833;
      --accent: #6c63ff;
      --accent2: #00d4aa;
      --text: #e0e0e0;
      --text-muted: #888;
      --border: #2a2d3a;
      --danger: #ff4757;
    }

    * { margin: 0; padding: 0; box-sizing: border-box; }

    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: var(--bg);
      color: var(--text);
      min-height: 100vh;
    }

    header {
      background: linear-gradient(135deg, var(--surface) 0%, var(--surface2) 100%);
      border-bottom: 1px solid var(--border);
      padding: 1.5rem 2rem;
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    header h1 {
      font-size: 1.5rem;
      font-weight: 600;
      background: linear-gradient(135deg, var(--accent), var(--accent2));
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    header .status {
      font-size: 0.85rem;
      color: var(--accent2);
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    header .status::before {
      content: '';
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--accent2);
      animation: pulse 2s infinite;
    }

    @keyframes pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.4; }
    }

    main {
      max-width: 1200px;
      margin: 2rem auto;
      padding: 0 2rem;
    }

    .metrics-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
      gap: 1rem;
      margin-bottom: 2rem;
    }

    .metric-card {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 1.25rem;
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
      transition: border-color 0.2s, transform 0.2s;
    }

    .metric-card:hover {
      border-color: var(--accent);
      transform: translateY(-2px);
    }

    .metric-value {
      font-size: 1.75rem;
      font-weight: 700;
      color: var(--accent);
      font-variant-numeric: tabular-nums;
    }

    .metric-label {
      font-size: 0.8rem;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }

    .tables-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 1.5rem;
    }

    @media (max-width: 768px) {
      .tables-row { grid-template-columns: 1fr; }
    }

    .table-section {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 12px;
      padding: 1.25rem;
    }

    .table-section h3 {
      font-size: 0.9rem;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      margin-bottom: 1rem;
    }

    table {
      width: 100%;
      border-collapse: collapse;
    }

    th {
      text-align: left;
      padding: 0.5rem;
      font-size: 0.75rem;
      color: var(--text-muted);
      border-bottom: 1px solid var(--border);
    }

    td {
      padding: 0.5rem;
      font-size: 0.85rem;
      border-bottom: 1px solid rgba(255,255,255,0.05);
      font-variant-numeric: tabular-nums;
    }

    tr:hover td {
      background: rgba(108, 99, 255, 0.05);
    }

    footer {
      text-align: center;
      padding: 2rem;
      color: var(--text-muted);
      font-size: 0.75rem;
    }
  </style>
</head>
<body>
  <header>
    <h1>🦀 QuectoClaw Dashboard</h1>
    <div class="status">Live — auto-refresh every 2s</div>
  </header>

  <main>
    <div id="metrics-panel"
         hx-get="/fragments/metrics"
         hx-trigger="load, every 2s"
         hx-swap="innerHTML">
      <div class="metrics-grid">
        <div class="metric-card">
          <span class="metric-value">--</span>
          <span class="metric-label">Loading...</span>
        </div>
      </div>
    </div>
  </main>

  <footer>
    QuectoClaw — Ultra-efficient AI Assistant
  </footer>
</body>
</html>"#;
