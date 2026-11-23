package io.github.perfectfriend.nexxus.mobile;

import android.annotation.SuppressLint;
import android.os.Bundle;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.view.WindowManager;
import androidx.appcompat.app.AppCompatActivity;

public class MainActivity extends AppCompatActivity {
    @SuppressLint("SetJavaScriptEnabled")
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // P1-14 (аудит 2026-11-23) WebView-харденинг:
        // FLAG_SECURE — запрет скриншотов/записи экрана (токен и секреты на экране).
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        WebView web = new WebView(this);
        web.getSettings().setJavaScriptEnabled(true);
        web.getSettings().setDomStorageEnabled(true);
        // Запрет доступа к файловой системе и content-провайдерам из WebView.
        web.getSettings().setAllowFileAccess(false);
        web.getSettings().setAllowContentAccess(false);
        web.getSettings().setAllowFileAccessFromFileURLs(false);
        web.getSettings().setAllowUniversalAccessFromFileURLs(false);
        // Только наш WebViewClient: внешние URL не открываются (никаких deeplink'ов наружу).
        web.setWebViewClient(new WebViewClient());
        web.loadUrl("file:///android_asset/www/index.html");
        setContentView(web);
    }
}
