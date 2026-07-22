package app.svitok.vault

import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.os.Bundle
import android.service.autofill.Dataset
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.view.autofill.AutofillId
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.RemoteViews
import android.widget.TextView
import androidx.core.content.res.ResourcesCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.fragment.app.FragmentActivity
import kotlin.concurrent.thread

// Диалог автозаполнения в виде нижнего листа - под палец, как остальной UI
// Свитка. Палитра, радиусы, шрифты и стили инпута/кнопки повторяют фронтенд.
// При открытии клавиатуры лист поднимается над ней (adjustResize + инсеты).
class AutofillAuthActivity : FragmentActivity() {

    companion object {
        const val EXTRA_SITE_LINE = "site_line"
        const val EXTRA_SITE_NAME = "site_name"
        const val EXTRA_KDF_M = "kdf_m"
        const val EXTRA_KDF_T = "kdf_t"
        const val EXTRA_USERNAME_ID = "username_id"
        const val EXTRA_PASSWORD_ID = "password_id"
    }

    // палитра фронтенда (styles.css :root)
    private val cBg = 0xFF141210.toInt()
    private val cSurface = 0xFF1C1916.toInt()
    private val cSurface2 = 0xFF26211B.toInt()
    private val cLine = 0xFF2E2822.toInt()
    private val cText = 0xDEEDE7DE.toInt()
    private val cText2 = 0x99EDE7DE.toInt()
    private val cText3 = 0x61EDE7DE.toInt()
    private val cSeal = 0xFFD4643E.toInt()
    private val cSealPress = 0xFFB84E2C.toInt()
    private val cOnSeal = 0xFF1A0F08.toInt()
    private val cErr = 0xFFC25B4E.toInt()

    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // экран показывает пароль - защищаем от скриншотов и записи
        window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)

        val siteLine = intent.getStringExtra(EXTRA_SITE_LINE).orEmpty()
        val siteName = intent.getStringExtra(EXTRA_SITE_NAME).orEmpty()
        val m = intent.getIntExtra(EXTRA_KDF_M, 20)
        val t = intent.getIntExtra(EXTRA_KDF_T, 21)
        @Suppress("DEPRECATION")
        val usernameId = intent.getParcelableExtra<AutofillId>(EXTRA_USERNAME_ID)
        @Suppress("DEPRECATION")
        val passwordId = intent.getParcelableExtra<AutofillId>(EXTRA_PASSWORD_ID)

        val golos = ResourcesCompat.getFont(this, R.font.golos)
        val piazzolla = ResourcesCompat.getFont(this, R.font.piazzolla)

        // подложка на весь экран, лист прижат книзу; тап по фону - отмена
        val scrim = FrameLayout(this).apply {
            setOnClickListener { finishCancel() }
        }

        val sheet = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(cSurface)
                cornerRadius = dp(24f)
            }
            isClickable = true // не проваливать тап сквозь карточку на scrim
            val padH = dp(22f).toInt()
            setPadding(padH, dp(22f).toInt(), padH, dp(22f).toInt())
        }

        val title = TextView(this).apply {
            text = siteName
            typeface = piazzolla ?: Typeface.SERIF
            textSize = 22f
            setTextColor(cText)
        }
        val caption = TextView(this).apply {
            text = "Мастер-фраза для автозаполнения"
            typeface = golos
            textSize = 14f
            setTextColor(cText2)
            setPadding(0, dp(4f).toInt(), 0, dp(16f).toInt())
        }

        val phrase = EditText(this).apply {
            hint = "Мастер-фраза"
            typeface = golos
            textSize = 16f
            setTextColor(cText)
            setHintTextColor(cText3)
            setPadding(dp(16f).toInt(), dp(12f).toInt(), dp(16f).toInt(), dp(12f).toInt())
            minHeight = dp(48f).toInt()
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
            importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_NO
            background = fieldBg(false)
            setOnFocusChangeListener { _, focused -> background = fieldBg(focused) }
        }

        // «Показать/Скрыть» справа под полем - как field-tools во фронтенде
        var revealed = false
        val reveal = TextView(this).apply {
            text = "Показать"
            typeface = golos
            textSize = 14f
            setTextColor(cText2)
            setPadding(dp(10f).toInt(), dp(6f).toInt(), dp(10f).toInt(), dp(6f).toInt())
            background = GradientDrawable().apply { setColor(cSurface2); cornerRadius = dp(8f) }
            setOnClickListener {
                revealed = !revealed
                val sel = phrase.selectionEnd
                phrase.inputType = InputType.TYPE_CLASS_TEXT or
                    if (revealed) InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
                    else InputType.TYPE_TEXT_VARIATION_PASSWORD
                phrase.typeface = golos // смена inputType сбрасывает шрифт
                phrase.setSelection(sel.coerceIn(0, phrase.text.length))
                text = if (revealed) "Скрыть" else "Показать"
            }
        }
        val tools = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.END
            setPadding(0, dp(8f).toInt(), 0, 0)
            addView(reveal)
        }

        val fill = Button(this).apply {
            text = "Заполнить"
            typeface = golos
            isAllCaps = false
            textSize = 16f
            setTextColor(cOnSeal)
            minHeight = dp(48f).toInt()
            stateListAnimator = null
            background = sealBtnBg()
        }

        val status = TextView(this).apply {
            typeface = golos
            textSize = 14f
            setTextColor(cText2)
            setPadding(0, dp(14f).toInt(), 0, 0)
        }

        sheet.addView(title)
        sheet.addView(caption)
        sheet.addView(phrase, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
        sheet.addView(tools)
        sheet.addView(fill, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(16f).toInt() })
        sheet.addView(status)

        val lp = FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT,
            FrameLayout.LayoutParams.WRAP_CONTENT,
            Gravity.CENTER,
        ).apply {
            val mh = dp(20f).toInt()
            marginStart = mh
            marginEnd = mh
        }
        scrim.addView(sheet, lp)
        setContentView(scrim)

        // карточка центрирована в безопасной зоне: сверху - статусбар, снизу -
        // максимум из навигации и клавиатуры. Открытие клавиатуры уменьшает зону,
        // и центр смещается вверх - модалка не перекрывается.
        ViewCompat.setOnApplyWindowInsetsListener(scrim) { v, insets ->
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            v.setPadding(0, bars.top, 0, maxOf(ime, bars.bottom))
            insets
        }

        // мягкое появление
        sheet.post {
            sheet.alpha = 0f
            sheet.scaleX = 0.94f
            sheet.scaleY = 0.94f
            sheet.animate().alpha(1f).scaleX(1f).scaleY(1f).setDuration(180).start()
        }

        fill.setOnClickListener {
            val ph = phrase.text.toString()
            if (ph.isEmpty()) {
                status.setTextColor(cErr)
                status.text = "Введите фразу"
                return@setOnClickListener
            }
            fill.isEnabled = false
            status.setTextColor(cText2)
            status.text = "Биометрия…"
            SeedStore.unlockSeedHex(this, { seedHex ->
                status.text = "Вывожу пароль…"
                thread {
                    val password = Native.derivePassword(seedHex, ph, m, t, siteLine)
                    runOnUiThread {
                        if (password.isEmpty()) {
                            status.setTextColor(cErr)
                            status.text = "Не вышло вывести пароль"
                            fill.isEnabled = true
                        } else {
                            finishWith(siteName, parseLogin(siteLine), password, usernameId, passwordId)
                        }
                    }
                }
            }, { err ->
                runOnUiThread {
                    status.setTextColor(cErr)
                    status.text = err
                    fill.isEnabled = true
                }
            })
        }
    }

    private fun fieldBg(focused: Boolean) = GradientDrawable().apply {
        setColor(cSurface2)
        cornerRadius = dp(12f)
        setStroke(dp(1f).toInt(), if (focused) cSeal else cLine)
    }

    private fun sealBtnBg(): StateListDrawable {
        fun round(color: Int) = GradientDrawable().apply { setColor(color); cornerRadius = dp(12f) }
        return StateListDrawable().apply {
            addState(intArrayOf(android.R.attr.state_pressed), round(cSealPress))
            addState(intArrayOf(-android.R.attr.state_enabled), round(cLine))
            addState(intArrayOf(), round(cSeal))
        }
    }

    private fun finishWith(
        siteName: String,
        login: String,
        password: String,
        usernameId: AutofillId?,
        passwordId: AutofillId?,
    ) {
        val presentation = RemoteViews(packageName, R.layout.autofill_row).apply {
            setTextViewText(R.id.af_text, "Свиток · $siteName")
        }
        val builder = Dataset.Builder(presentation)
        if (usernameId != null && login.isNotEmpty()) {
            builder.setValue(usernameId, AutofillValue.forText(login), presentation)
        }
        if (passwordId != null) {
            builder.setValue(passwordId, AutofillValue.forText(password), presentation)
        }
        val reply = Intent().apply {
            putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, builder.build())
        }
        setResult(RESULT_OK, reply)
        finish()
    }

    private fun finishCancel() {
        setResult(RESULT_CANCELED)
        finish()
    }

    // login=... из строки списка; пусто, если логина нет
    private fun parseLogin(siteLine: String): String {
        for (tok in siteLine.trim().split(Regex("\\s+"))) {
            if (tok.startsWith("login=")) return tok.removePrefix("login=")
        }
        return ""
    }

    // держим Color импортированным осмысленно: прозрачный фон окна-подложки
    override fun onStart() {
        super.onStart()
        window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(Color.TRANSPARENT))
    }

    override fun onPause() {
        super.onPause()
        if (!isFinishing) setResult(RESULT_CANCELED)
    }
}
