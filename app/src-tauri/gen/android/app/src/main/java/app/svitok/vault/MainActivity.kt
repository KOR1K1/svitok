package app.svitok.vault

import android.annotation.SuppressLint
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import kotlin.math.roundToInt

class MainActivity : TauriActivity() {
    private var webView: WebView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        // на экране секреты, поэтому скриншоты и запись экрана уходят в чёрноту
        window.setFlags(
            WindowManager.LayoutParams.FLAG_SECURE,
            WindowManager.LayoutParams.FLAG_SECURE,
        )

        // тема «Чернила» тёмная, значит иконки баров светлые и без скрима на навбаре
        WindowInsetsControllerCompat(window, window.decorView).apply {
            isAppearanceLightStatusBars = false
            isAppearanceLightNavigationBars = false
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            window.isNavigationBarContrastEnforced = false
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }

        // «назад» (кнопка или жест) отдаём в JS: закрыть sheet, вернуться на вкладку.
        // приложение закроется, только когда JS сам решит, что идти уже некуда.
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                webView?.evaluateJavascript(
                    "window.__svitokBack ? window.__svitokBack() " +
                        ": (window.__svitokAndroid && window.__svitokAndroid.exit())",
                    null,
                )
            }
        })
    }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
        webView.setBackgroundColor(0x00000000) // прозрачный, чтобы бары просвечивали

        // мост для выхода: JS дёргает его, когда возвращаться уже некуда
        webView.addJavascriptInterface(object {
            @JavascriptInterface
            fun exit() {
                runOnUiThread { finish() }
            }
        }, "__svitokAndroid")

        // инсеты кладём в CSS-переменные сами. на env() не полагаемся:
        // в старых WebView он врёт или отдаёт ноль. клавиатура входит в bottom.
        val density = resources.displayMetrics.density
        ViewCompat.setOnApplyWindowInsetsListener(webView) { _, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val cut = insets.getInsets(WindowInsetsCompat.Type.displayCutout())
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            fun px(v: Int) = (v / density).roundToInt()
            val top = px(maxOf(bars.top, cut.top))
            val bottom = px(maxOf(bars.bottom, cut.bottom, ime.bottom))
            val left = px(maxOf(bars.left, cut.left))
            val right = px(maxOf(bars.right, cut.right))
            webView.evaluateJavascript(
                """
                (function(s){
                  s.setProperty('--safe-area-inset-top','${top}px');
                  s.setProperty('--safe-area-inset-bottom','${bottom}px');
                  s.setProperty('--safe-area-inset-left','${left}px');
                  s.setProperty('--safe-area-inset-right','${right}px');
                })(document.documentElement.style);
                """.trimIndent(),
                null,
            )
            insets
        }
    }
}
