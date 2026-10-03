<div align="center">

<h1>Mosaik</h1>

<h3>Workspace organizer that adapts to workflow patterns</h3>

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://rust-lang.org)
[![Python](https://img.shields.io/badge/Python-3776AB?style=for-the-badge&logo=python&logoColor=white)](https://python.org)
[![React](https://img.shields.io/badge/React-61DAFB?style=for-the-badge&logo=react&logoColor=black)](https://reactjs.org)

<hr>

<h2>About</h2>

<p>Mosaik watches how you actually work and reorganizes your workspace around those patterns.<br>
If you keep opening the same three files whenever you start on a specific project,<br>
it notices, and the next time you start that kind of work it sets things up for you.</p>

<p>Files you use together get grouped. Apps you reach for in sequence become a workflow.<br>
Daily rhythms get picked up on, so the workspace you get in the morning<br>
is different from the one in the evening.</p>

<p><b>Everything runs locally. No accounts, no telemetry, no data leaving your machine.</b></p>

<hr>

<h2>Patterns</h2>

<table align="center">
<tr>
<td align="center" width="33%">

<h3>Files</h3>

Groups related files<br>
Detects project context

</td>
<td align="center" width="33%">

<h3>Apps</h3>

Learns tool combinations<br>
Builds workflow profiles

</td>
<td align="center" width="33%">

<h3>Time</h3>

Daily rhythms<br>
Focus periods

</td>
</tr>
</table>

<hr>

<h2>Example</h2>

```python
if user.opens("VS Code") and user.recent_files(".py"):
    mosaik.suggest_workspace("Python Project")
    mosaik.group_files(["*.py", "*.txt", "*.md"])
    mosaik.recommend_apps(["Terminal", "Browser"])
```

<hr>

<h2>Stack</h2>

```javascript
const tech = {
  core: "Rust",
  ai: "Python + scikit-learn",
  ui: "Tauri + React",
  storage: "Local SQLite"
};
```

<hr>

<h2>Install</h2>

```bash
# macOS/Linux
curl -fsSL https://mosaik.dev/install.sh | sh

# Windows
powershell -c "irm mosaik.dev/install.ps1 | iex"
```

<hr>

[![Download](https://img.shields.io/badge/Download-000000?style=for-the-badge&logo=download&logoColor=white)](https://mosaik.dev)

</div>
