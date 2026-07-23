package app.svitok.vault

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.os.Build
import android.os.Bundle
import android.os.VibrationEffect
import android.os.Vibrator
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.core.content.res.ResourcesCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

// Свой сканер QR: CameraX + ZXing-core, без Google ML Kit и его телеметрии.
// UX по мотивам Telegram: затемнение с рамкой-видоискателем, срабатывает только
// внутри рамки, найденный код рамка «ловит» - плавно летит к нему и держится
// при движении камеры; после устойчивого захвата - вибрация и результат.
class ScannerActivity : ComponentActivity() {

    companion object {
        const val EXTRA_HINT = "hint"

        // ожидающий вызов из KeystorePlugin.scanQr; активность отвечает ровно раз
        var pending: Invoke? = null

        fun deliver(content: String?, error: String = "cancelled") {
            val inv = pending ?: return
            pending = null
            if (content == null) {
                inv.reject(error)
            } else {
                val r = JSObject()
                r.put("value", content)
                inv.resolve(r)
            }
        }
    }

    // палитра фронтенда (styles.css :root), как в AutofillAuthActivity
    private val cSurface = 0xFF1C1916.toInt()
    private val cText = 0xDEEDE7DE.toInt()
    private val cSeal = 0xFFD4643E.toInt()

    private lateinit var overlay: ScanOverlay
    private lateinit var previewView: PreviewView
    private var analysisExec: ExecutorService? = null
    private var done = false

    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // в кадре может быть секрет (otpauth-QR) - не даём записать экран
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        val root = FrameLayout(this)
        previewView = PreviewView(this).apply { scaleType = PreviewView.ScaleType.FILL_CENTER }
        val preview = previewView
        overlay = ScanOverlay(this)

        val golos = ResourcesCompat.getFont(this, R.font.golos)
        val hint = TextView(this).apply {
            text = intent.getStringExtra(EXTRA_HINT).orEmpty()
            typeface = golos
            textSize = 15f
            setTextColor(cText)
            gravity = Gravity.CENTER
        }
        val close = TextView(this).apply {
            text = "✕"
            typeface = golos ?: Typeface.DEFAULT
            textSize = 20f
            setTextColor(cText)
            gravity = Gravity.CENTER
            background = android.graphics.drawable.GradientDrawable().apply {
                setColor(cSurface)
                cornerRadius = dp(22f)
            }
            setOnClickListener { finishCancel() }
        }

        root.addView(preview, FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT)
        root.addView(overlay, FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT)
        root.addView(close, FrameLayout.LayoutParams(dp(44f).toInt(), dp(44f).toInt()))
        root.addView(hint, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.WRAP_CONTENT))
        setContentView(root)

        // кнопка - под статус-баром, подсказка - под рамкой
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            (close.layoutParams as FrameLayout.LayoutParams).setMargins(
                dp(16f).toInt(), bars.top + dp(12f).toInt(), 0, 0,
            )
            close.requestLayout()
            insets
        }
        overlay.onBaseRect = { base ->
            (hint.layoutParams as FrameLayout.LayoutParams).topMargin = (base.bottom + dp(24f)).toInt()
            hint.requestLayout()
        }

        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            startCamera(preview)
        } else {
            requestPermissions(arrayOf(Manifest.permission.CAMERA), 7)
        }
    }

    override fun onRequestPermissionsResult(code: Int, perms: Array<String>, res: IntArray) {
        super.onRequestPermissionsResult(code, perms, res)
        if (code != 7) return
        if (res.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            startCamera(previewView)
        } else {
            done = true
            deliver(null, "no-camera")
            finish()
        }
    }

    private fun startCamera(previewView: PreviewView) {
        val future = ProcessCameraProvider.getInstance(this)
        future.addListener({
            val provider = future.get()
            val preview = Preview.Builder().build().also { it.surfaceProvider = previewView.surfaceProvider }
            val analysis = ImageAnalysis.Builder()
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
            analysisExec = Executors.newSingleThreadExecutor()
            val reader = MultiFormatReader().apply {
                setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
            }
            analysis.setAnalyzer(analysisExec!!) { img -> decodeFrame(reader, img) }
            try {
                provider.unbindAll()
                provider.bindToLifecycle(this, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
            } catch (e: Exception) {
                done = true
                deliver(null, "no-camera")
                finish()
            }
        }, ContextCompat.getMainExecutor(this))
    }

    private fun decodeFrame(reader: MultiFormatReader, img: ImageProxy) {
        img.use {
            if (done) return
            // ZXing хочет плотную яркостную плоскость - Y-плоскость копируем построчно,
            // потому что rowStride у камеры обычно шире самой картинки
            val plane = it.planes[0]
            val w = it.width
            val h = it.height
            val data = ByteArray(w * h)
            val buf = plane.buffer
            if (plane.rowStride == w) {
                buf.get(data, 0, w * h)
            } else {
                var off = 0
                for (row in 0 until h) {
                    buf.position(row * plane.rowStride)
                    buf.get(data, off, w)
                    off += w
                }
            }
            val source = PlanarYUVLuminanceSource(data, w, h, 0, 0, w, h, false)
            val result = try {
                reader.decodeWithState(BinaryBitmap(HybridBinarizer(source)))
            } catch (_: Exception) {
                null
            } finally {
                reader.reset()
            }
            if (result != null) {
                val pts = result.resultPoints ?: return
                val rect = mapToView(pts.map { p -> floatArrayOf(p.x, p.y) }, w, h, it.imageInfo.rotationDegrees)
                runOnUiThread { overlay.onDetected(rect, result.text) }
            }
        }
    }

    /// Точки кадра -> координаты оверлея: поворот кадра, затем FILL_CENTER
    /// (масштаб по большей стороне и центрирование, как у PreviewView).
    private fun mapToView(points: List<FloatArray>, imgW: Int, imgH: Int, rotation: Int): RectF {
        val vw = overlay.width.toFloat()
        val vh = overlay.height.toFloat()
        val rw: Float
        val rh: Float
        if (rotation == 90 || rotation == 270) {
            rw = imgH.toFloat(); rh = imgW.toFloat()
        } else {
            rw = imgW.toFloat(); rh = imgH.toFloat()
        }
        val scale = maxOf(vw / rw, vh / rh)
        val dx = (vw - rw * scale) / 2f
        val dy = (vh - rh * scale) / 2f
        var minX = Float.MAX_VALUE; var minY = Float.MAX_VALUE
        var maxX = -Float.MAX_VALUE; var maxY = -Float.MAX_VALUE
        for (p in points) {
            val (x, y) = p
            val rx: Float
            val ry: Float
            when (rotation) {
                90 -> { rx = imgH - 1f - y; ry = x }
                180 -> { rx = imgW - 1f - x; ry = imgH - 1f - y }
                270 -> { rx = y; ry = imgW - 1f - x }
                else -> { rx = x; ry = y }
            }
            val vx = rx * scale + dx
            val vy = ry * scale + dy
            if (vx < minX) minX = vx
            if (vy < minY) minY = vy
            if (vx > maxX) maxX = vx
            if (vy > maxY) maxY = vy
        }
        val r = RectF(minX, minY, maxX, maxY)
        // resultPoints - это finder-узоры, сам код чуть больше их габарита
        r.inset(-r.width() * 0.18f, -r.height() * 0.18f)
        return r
    }

    private fun success(content: String) {
        if (done) return
        done = true
        val vib = getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        if (Build.VERSION.SDK_INT >= 29) {
            vib?.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_CLICK))
        } else {
            @Suppress("DEPRECATION") vib?.vibrate(30)
        }
        deliver(content)
        // даём рамке долететь - выглядит как «поймал», а не как обрыв
        overlay.postDelayed({ finish() }, 160L)
    }

    private fun finishCancel() {
        if (!done) {
            done = true
            deliver(null)
        }
        finish()
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        finishCancel()
    }

    override fun onDestroy() {
        super.onDestroy()
        analysisExec?.shutdown()
        // системное закрытие (свернули, убили) - отвечаем отменой, не висим
        if (!done) deliver(null)
    }

    // Затемнение с вырезом-рамкой. Рамка плавно гонится за целью (найденным QR):
    // каждый кадр подтягивается на четверть расстояния - выходит телеграмный
    // «прилип и держится». Захват засчитываем после ~полусекунды устойчивого
    // ведения одного и того же кода.
    private inner class ScanOverlay(ctx: Context) : View(ctx) {
        var onBaseRect: ((RectF) -> Unit)? = null

        private val scrim = Paint().apply { color = 0xA6000000.toInt() }
        private val corner = Paint().apply {
            color = cSeal
            style = Paint.Style.STROKE
            strokeWidth = dp(4f)
            strokeCap = Paint.Cap.ROUND
            isAntiAlias = true
        }
        private val path = Path()

        private val base = RectF()
        private val cur = RectF()
        private val target = RectF()
        private var hasTarget = false
        private var lockText: String? = null
        private var lockSince = 0L
        private var lastSeen = 0L

        override fun onSizeChanged(w: Int, h: Int, ow: Int, oh: Int) {
            val side = minOf(w, h) * 0.66f
            val cx = w / 2f
            val cy = h * 0.44f
            base.set(cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2)
            cur.set(base)
            target.set(base)
            onBaseRect?.invoke(base)
        }

        fun onDetected(rect: RectF, text: String) {
            // ловим только то, что в зоне рамки (с запасом) - а не весь кадр
            val zone = RectF(base).apply { inset(-base.width() * 0.3f, -base.height() * 0.3f) }
            if (!zone.contains(rect.centerX(), rect.centerY())) return
            val now = android.os.SystemClock.uptimeMillis()
            if (text != lockText) {
                lockText = text
                lockSince = now
            }
            target.set(rect)
            hasTarget = true
            lastSeen = now
            postInvalidateOnAnimation()
        }

        override fun onDraw(canvas: Canvas) {
            val now = android.os.SystemClock.uptimeMillis()
            // потеряли код из виду - плавно возвращаемся к базовой рамке
            if (hasTarget && now - lastSeen > 600) {
                hasTarget = false
                lockText = null
                target.set(base)
            }
            // погоня за целью: каждый кадр четверть пути
            cur.left += (target.left - cur.left) * 0.25f
            cur.top += (target.top - cur.top) * 0.25f
            cur.right += (target.right - cur.right) * 0.25f
            cur.bottom += (target.bottom - cur.bottom) * 0.25f

            val radius = dp(24f)
            path.reset()
            path.fillType = Path.FillType.EVEN_ODD
            path.addRect(0f, 0f, width.toFloat(), height.toFloat(), Path.Direction.CW)
            path.addRoundRect(cur, radius, radius, Path.Direction.CW)
            canvas.drawPath(path, scrim)

            // четыре уголка по углам рамки
            val len = minOf(cur.width(), cur.height()) * 0.22f
            val r = radius * 0.9f
            drawCornerArcs(canvas, cur, r, len)

            val settled = hasTarget &&
                kotlin.math.abs(cur.left - target.left) + kotlin.math.abs(cur.right - target.right) < dp(6f)
            if (settled && lockText != null && now - lockSince > 450) {
                success(lockText!!)
            }
            if (hasTarget || kotlin.math.abs(cur.left - base.left) > 0.5f) {
                postInvalidateOnAnimation()
            }
        }

        private fun drawCornerArcs(canvas: Canvas, rc: RectF, r: Float, len: Float) {
            // каждый угол: дуга скругления и два уса вдоль сторон
            fun line(x1: Float, y1: Float, x2: Float, y2: Float) = canvas.drawLine(x1, y1, x2, y2, corner)
            // левый верхний
            canvas.drawArc(rc.left, rc.top, rc.left + 2 * r, rc.top + 2 * r, 180f, 90f, false, corner)
            line(rc.left + r, rc.top, rc.left + len, rc.top)
            line(rc.left, rc.top + r, rc.left, rc.top + len)
            // правый верхний
            canvas.drawArc(rc.right - 2 * r, rc.top, rc.right, rc.top + 2 * r, 270f, 90f, false, corner)
            line(rc.right - len, rc.top, rc.right - r, rc.top)
            line(rc.right, rc.top + r, rc.right, rc.top + len)
            // правый нижний
            canvas.drawArc(rc.right - 2 * r, rc.bottom - 2 * r, rc.right, rc.bottom, 0f, 90f, false, corner)
            line(rc.right, rc.bottom - len, rc.right, rc.bottom - r)
            line(rc.right - r, rc.bottom, rc.right - len, rc.bottom)
            // левый нижний
            canvas.drawArc(rc.left, rc.bottom - 2 * r, rc.left + 2 * r, rc.bottom, 90f, 90f, false, corner)
            line(rc.left + r, rc.bottom, rc.left + len, rc.bottom)
            line(rc.left, rc.bottom - len, rc.left, rc.bottom - r)
        }
    }
}
