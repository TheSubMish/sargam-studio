
// ------------------------------------------------------------------ auth ---
//
// Real Supabase Auth: auth.rs calls Supabase's GoTrue REST API over HTTPS
// to sign up/log in/log out. Projects (.song.json files) are unaffected and
// stay local — this only covers the account/sign-in layer.

let authStatus = { hasAccount: false, isAuthenticated: false, name: "", email: "" };

function authInitials() {
  return authStatus.name
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .join("") || "?";
}

async function refreshAuthStatus() {
  authStatus = await invoke("get_auth_status");
}

function openAuthModal() {
  closeSettingsDrawer(); // the login modal shouldn't sit on top of an open drawer
  el("auth-error").classList.add("hidden");
  el("auth-login-email").value = "";
  el("auth-login-password").value = "";
  el("auth-signup-name").value = "";
  el("auth-signup-email").value = "";
  el("auth-signup-password").value = "";
  const startOnSignup = !authStatus.hasAccount;
  setAuthMode(startOnSignup ? "signup" : "login");
  el("auth-backdrop").classList.remove("hidden");
}

function setAuthMode(mode) {
  el("auth-title").textContent = mode === "login" ? "Log in" : "Sign up";
  el("auth-login-fields").classList.toggle("hidden", mode !== "login");
  el("auth-signup-fields").classList.toggle("hidden", mode !== "signup");
  el("auth-error").classList.add("hidden");
}

function showAuthError(message) {
  const box = el("auth-error");
  box.textContent = message;
  box.classList.remove("hidden");
}

function setupAuthModal() {
  el("auth-cancel").addEventListener("click", () => el("auth-backdrop").classList.add("hidden"));
  el("auth-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "auth-backdrop") el("auth-backdrop").classList.add("hidden");
  });
  el("auth-to-signup").addEventListener("click", () => setAuthMode("signup"));
  el("auth-to-login").addEventListener("click", () => setAuthMode("login"));

  el("auth-do-login").addEventListener("click", async () => {
    try {
      authStatus = await invoke("login", {
        email: el("auth-login-email").value,
        password: el("auth-login-password").value,
      });
      el("auth-backdrop").classList.add("hidden");
      renderDrawerContent();
      showToast(`Logged in as ${authStatus.name}`, "ok");
    } catch (err) {
      showAuthError(String(err));
    }
  });

  el("auth-do-signup").addEventListener("click", async () => {
    try {
      authStatus = await invoke("signup", {
        name: el("auth-signup-name").value,
        email: el("auth-signup-email").value,
        password: el("auth-signup-password").value,
      });
      el("auth-backdrop").classList.add("hidden");
      renderDrawerContent();
      showToast(`Account created — welcome, ${authStatus.name}`, "ok");
    } catch (err) {
      showAuthError(String(err));
    }
  });
}

async function doLogout() {
  if (!confirm("Log out?")) return;
  authStatus = await invoke("logout");
  renderDrawerContent();
}
