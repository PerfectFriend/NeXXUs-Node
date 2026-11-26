package io.github.perfectfriend.nexxus.mobile;

import android.content.Context;
import android.content.SharedPreferences;
import android.webkit.JavascriptInterface;

import androidx.security.crypto.EncryptedSharedPreferences;
import androidx.security.crypto.MasterKey;

/**
 * P1-14 (аудит 2026-11-23): API-токен хранится в EncryptedSharedPreferences,
 * master key — в Android Keystore. Токен не лежит в открытом виде в assets/префах.
 */
public class TokenStore {
    private static final String PREFS = "nexxus_secure";
    private static final String KEY_TOKEN = "api_token";
    private final SharedPreferences prefs;

    public TokenStore(Context ctx) {
        try {
            MasterKey masterKey = new MasterKey.Builder(ctx)
                    .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
                    .build();
            this.prefs = EncryptedSharedPreferences.create(
                    ctx, PREFS, masterKey,
                    EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                    EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM);
        } catch (Exception e) {
            throw new IllegalStateException("cannot init EncryptedSharedPreferences", e);
        }
    }

    public String getToken() {
        String t = prefs.getString(KEY_TOKEN, "");
        return t == null ? "" : t;
    }

    public void setToken(String token) {
        prefs.edit().putString(KEY_TOKEN, token).apply();
    }

    public void clearToken() {
        prefs.edit().remove(KEY_TOKEN).apply();
    }

    /** JS-мост для WebView: window.Android.getToken()/setToken(t)/clearToken() */
    public static class Bridge {
        private final TokenStore store;
        public Bridge(TokenStore store) { this.store = store; }

        @JavascriptInterface
        public String getToken() { return store.getToken(); }

        @JavascriptInterface
        public void setToken(String token) { store.setToken(token == null ? "" : token.trim()); }

        @JavascriptInterface
        public void clearToken() { store.clearToken(); }
    }
}
