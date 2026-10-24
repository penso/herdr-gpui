// Picks one variant per row and hands them to the agent through Herdr UI.
const picks = {};
const label = document.getElementById("picks");
const describe = () => Object.entries(picks).map(([k, v]) => `${k}: ${v}`).join(" · ");

const chosen = { home: "F", navigation: "D", chat: "C", question: "A" };
Object.assign(picks, chosen);
for (const row of document.querySelectorAll(".row[data-group]")) {
  row.querySelector(`figure[data-variant="${chosen[row.dataset.group]}"]`)?.classList.add("picked");
  for (const figure of row.querySelectorAll("figure[data-variant]")) {
    const button = document.createElement("button");
    button.className = "hk-button pick";
    button.textContent = "Pick " + figure.dataset.variant;
    button.addEventListener("click", () => {
      row.querySelectorAll("figure").forEach((f) => f.classList.remove("picked"));
      figure.classList.add("picked");
      picks[row.dataset.group] = figure.dataset.variant;
      label.textContent = describe();
    });
    figure.appendChild(button);
  }
}

document.getElementById("send").addEventListener("click", () => {
  const summary = "Remote control picks: " + (describe() || "none");
  if (window.herdr && window.herdr.send(summary, picks)) return;
  navigator.clipboard?.writeText(summary + " " + JSON.stringify(picks));
  label.textContent = "Copied to clipboard (not in Herdr UI).";
});

document.getElementById("theme").addEventListener("click", () => {
  document.documentElement.classList.toggle("light");
});

label.textContent = describe();
