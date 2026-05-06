/* Progressive enhancement: replace each platform card's href with a direct
 * download URL pulled from the latest GitHub Release. Falls back to the
 * /releases/latest page if the API is unreachable.
 *
 * Pure DOM, no innerHTML for untrusted strings.
 */
const REPO = "emerleite/doppelganger";

const matchers = {
  "macos-arm":  /aarch64.*\.dmg$/i,
  "macos-x64":  /x64.*\.dmg$/i,
  "windows":    /\.exe$|setup.*\.exe$|\.msi$/i,
  "linux":      /\.AppImage$/i,
};

async function loadLatestRelease() {
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return await res.json();
  } catch (err) {
    console.warn("Could not fetch latest release; falling back to releases page.", err);
    return null;
  }
}

function pickAsset(assets, regex) {
  return assets.find((a) => regex.test(a.name));
}

(async function () {
  const release = await loadLatestRelease();
  if (!release) return;

  // Update the version badge
  const badge = document.getElementById("version-badge");
  if (badge && release.tag_name) badge.textContent = release.tag_name;

  // Wire each platform card to its actual asset URL
  for (const [platform, rx] of Object.entries(matchers)) {
    const card = document.querySelector(`.platform[data-platform="${platform}"]`);
    if (!card) continue;
    const asset = pickAsset(release.assets || [], rx);
    if (asset) {
      card.href = asset.browser_download_url;
      const action = card.querySelector(".platform-action");
      if (action) {
        const sizeMb = (asset.size / 1_048_576).toFixed(1);
        action.textContent = `Download · ${sizeMb} MB`;
      }
    }
  }
})();
