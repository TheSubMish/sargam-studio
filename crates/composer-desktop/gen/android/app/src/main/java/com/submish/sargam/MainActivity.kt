package com.submish.sargam

import android.os.Bundle

// Tauri's generated MainActivity calls enableEdgeToEdge(), which draws the
// WebView *under* the status/navigation bars. The insets don't propagate to
// CSS env(safe-area-inset-*) in this WebView, so our bottom nav ended up
// hidden behind the Android navigation bar. Dropping edge-to-edge lets the
// window fit the system bars, so content sits cleanly between them.
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
  }
}
