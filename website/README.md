# Sargam website

Static marketing site + legal pages for Sargam. Plain HTML/CSS, no build step —
open `index.html` locally or deploy the folder as-is.

## Pages

| File | Purpose |
|---|---|
| `index.html` | Marketing landing (desktop + Android) |
| `privacy.html` | Privacy Policy *(required by Google Play)* |
| `delete-account.html` | Account & data deletion *(required by Google Play)* |
| `terms.html` | Terms of Service / EULA |
| `content-policy.html` | Content & Copyright / takedown *(required for user uploads)* |
| `seller-terms.html` | Seller / payout terms |

## Before you publish — fill in the placeholders

Every `[BRACKETED]` value is a placeholder. Search the folder and replace:

- `[Your Legal Name / Entity]`
- `[SUPPORT_EMAIL]`, `[PRIVACY_EMAIL]`, `[ABUSE_EMAIL]`, `[COPYRIGHT_EMAIL]`
- `[PLAY_STORE_URL]`, `[DESKTOP_DOWNLOAD_URL]` (e.g. your GitHub Releases page)
- Effective dates, retention period, revenue split, refund terms, jurisdiction

```bash
grep -rn '\[' website/*.html   # list remaining placeholders
```

> The legal pages are **starting templates, not legal advice.** Because Sargam
> handles accounts, payments, payouts, and user-uploaded content, have a lawyer
> review them before launch.

## Deploy

### Option A — GitHub Pages (workflow included)

1. Push this repo to GitHub.
2. Repo **Settings → Pages → Build and deployment → Source: GitHub Actions**.
3. The included workflow (`.github/workflows/deploy-website.yml`) publishes
   `website/` on every push to `main` that touches it. The live URL appears in
   the Actions run and under Settings → Pages.

Custom domain: add it under Settings → Pages, or drop a `CNAME` file in this
folder.

### Option B — Vercel

1. Import the repo at [vercel.com/new](https://vercel.com/new).
2. Set **Root Directory** to `website`, **Framework Preset** to *Other*, and
   leave the build command empty (it's a static site).
3. Deploy. `vercel.json` enables clean URLs (`/privacy` instead of
   `/privacy.html`).

Or from the CLI:

```bash
npm i -g vercel
cd website
vercel        # preview
vercel --prod # production
```
