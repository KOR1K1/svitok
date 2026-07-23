package app.svitok.vault

import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.InlinePresentation
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import android.text.InputType
import android.util.Log
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import android.widget.inline.InlinePresentationSpec
import androidx.annotation.RequiresApi
import androidx.autofill.inline.UiVersions
import androidx.autofill.inline.v1.InlineSuggestionUi
import java.io.File

// Автозаполнение логина и пароля. Пароли не хранятся, а выводятся, поэтому
// каждый вариант - это "authentication dataset": при выборе система запускает
// AutofillAuthActivity, где идёт биометрия + фраза + деривация одного пароля.
// Сам сервис секретов не касается: только сопоставляет домен со списком сайтов.
class SvitokAutofillService : AutofillService() {

    private companion object {
        const val TAG = "SvitokAF"
    }

    private data class Fields(
        val usernameId: AutofillId?,
        val passwordId: AutofillId?,
        val webDomain: String?,
    )

    private data class SiteLine(
        val name: String,
        val login: String,
        val aliases: List<String>,
        val label: String,
        val raw: String,
    ) {
        // подпись варианта: label вместо name, если задан; логин различает
        // несколько аккаунтов на одном домене
        fun display(): String {
            val base = label.ifEmpty { name }
            return if (login.isEmpty()) base else "$base ($login)"
        }
    }

    private data class Sites(val m: Int, val t: Int, val lines: List<SiteLine>)

    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
      try {
        val structure = request.fillContexts.lastOrNull()?.structure
        if (structure == null) {
            callback.onSuccess(null)
            return
        }
        val fields = parseStructure(structure)
        if (fields.passwordId == null && fields.usernameId == null) {
            callback.onSuccess(null)
            return
        }
        // домен: у браузеров - webDomain из структуры, у приложений - имя пакета
        val rawDomain = fields.webDomain ?: structure.activityComponent?.packageName
        val canon = rawDomain?.let { Native.canonicalDomain(it) }.orEmpty()

        val sites = readSites()
        if (sites == null || sites.lines.isEmpty()) {
            callback.onSuccess(null)
            return
        }
        // запись матчится по name и по каждому alias; алиасы - только матчинг,
        // пароль всегда выводится из name (он в строке raw)
        val matches = sites.lines.filter { site ->
            val domains = listOf(site.name) + site.aliases
            domains.any { d ->
                val c = Native.canonicalDomain(d)
                (c.isNotEmpty() && c == canon) || (canon.isEmpty() && rawDomain != null && d.equals(rawDomain, true))
            }
        }
        if (matches.isEmpty()) {
            callback.onSuccess(null)
            return
        }

        // inline-подсказки (чипы прямо в клавиатуре, как у Google); если клавиатура
        // их не поддерживает - остаётся menu-презентация (RemoteViews)
        val inlineSpecs = if (Build.VERSION.SDK_INT >= 30) {
            request.inlineSuggestionsRequest?.inlinePresentationSpecs
        } else {
            null
        }

        val response = FillResponse.Builder()
        val ids = listOfNotNull(fields.usernameId, fields.passwordId)
        for ((i, site) in matches.withIndex()) {
            val label = "Свиток · ${site.display()}"
            val menu = RemoteViews(packageName, R.layout.autofill_row).apply {
                setTextViewText(R.id.af_text, label)
            }
            val inline = if (Build.VERSION.SDK_INT >= 30 && inlineSpecs != null && i < inlineSpecs.size) {
                buildInline(inlineSpecs[i], label)
            } else {
                null
            }
            val builder = Dataset.Builder()
            for (id in ids) {
                if (Build.VERSION.SDK_INT >= 30 && inline != null) {
                    builder.setValue(id, null, menu, inline)
                } else {
                    builder.setValue(id, null, menu)
                }
            }
            builder.setAuthentication(authSender(site, sites.m, sites.t, fields, i))
            response.addDataset(builder.build())
        }
        callback.onSuccess(response.build())
      } catch (e: Throwable) {
        Log.e(TAG, "onFillRequest упал", e)
        callback.onSuccess(null)
      }
    }

    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        // сохранять нечего: пароли не хранятся, сайты пользователь заводит в приложении
        callback.onSuccess()
    }

    private fun authSender(site: SiteLine, m: Int, t: Int, fields: Fields, index: Int): android.content.IntentSender {
        val intent = Intent(this, AutofillAuthActivity::class.java).apply {
            putExtra(AutofillAuthActivity.EXTRA_SITE_LINE, site.raw)
            putExtra(AutofillAuthActivity.EXTRA_SITE_NAME, site.display())
            putExtra(AutofillAuthActivity.EXTRA_KDF_M, m)
            putExtra(AutofillAuthActivity.EXTRA_KDF_T, t)
            putExtra(AutofillAuthActivity.EXTRA_USERNAME_ID, fields.usernameId)
            putExtra(AutofillAuthActivity.EXTRA_PASSWORD_ID, fields.passwordId)
        }
        // с API 31 IntentSender для авторизации должен быть mutable, чтобы система
        // дописала в него результат; на 26-30 PendingIntent изменяем по умолчанию
        val flags = if (Build.VERSION.SDK_INT >= 31) {
            PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_MUTABLE
        } else {
            PendingIntent.FLAG_CANCEL_CURRENT
        }
        return PendingIntent.getActivity(this, index, intent, flags).intentSender
    }

    @RequiresApi(30)
    private fun buildInline(spec: InlinePresentationSpec, title: String): InlinePresentation? {
        return try {
            // клавиатура сообщает поддерживаемую версию UI; если не наша - не рискуем
            if (!UiVersions.getVersions(spec.style).contains(UiVersions.INLINE_UI_VERSION_1)) {
                return null
            }
            // attribution-intent обязателен (по долгому тапу по чипу) - ведём в приложение
            val attribution = PendingIntent.getActivity(
                this, 1000, Intent(this, MainActivity::class.java),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            val content = InlineSuggestionUi.newContentBuilder(attribution)
                .setTitle(title)
                .setStartIcon(Icon.createWithResource(this, R.mipmap.ic_launcher))
                .build()
            InlinePresentation(content.slice, spec, false)
        } catch (e: Throwable) {
            Log.e(TAG, "inline не построился", e)
            null
        }
    }

    // --- разбор структуры ---

    private fun parseStructure(structure: AssistStructure): Fields {
        var username: AutofillId? = null
        var password: AutofillId? = null
        var domain: String? = null
        for (i in 0 until structure.windowNodeCount) {
            val root = structure.getWindowNodeAt(i).rootViewNode
            domain = domain ?: findDomain(root)
            traverse(root) { node ->
                val id = node.autofillId ?: return@traverse
                when (classify(node)) {
                    Kind.PASSWORD -> if (password == null) password = id
                    Kind.USERNAME -> if (username == null) username = id
                    Kind.NONE -> {}
                }
            }
        }
        return Fields(username, password, domain)
    }

    private enum class Kind { USERNAME, PASSWORD, NONE }

    // Классифицируем поле по трём источникам: autofillHints (нативные приложения),
    // htmlInfo (веб-формы в браузере - там тип в <input type=...>), и типу поля.
    private fun classify(node: AssistStructure.ViewNode): Kind {
        node.autofillHints?.forEach { h ->
            when (h.lowercase()) {
                "password", "current-password", "new-password" -> return Kind.PASSWORD
                "username", "emailaddress", "email" -> return Kind.USERNAME
            }
        }
        val html = node.htmlInfo
        if (html != null && html.tag.equals("input", ignoreCase = true)) {
            var type = ""
            var ac = ""
            var name = ""
            html.attributes?.forEach { pair ->
                when (pair.first.lowercase()) {
                    "type" -> type = pair.second.lowercase()
                    "autocomplete" -> ac = pair.second.lowercase()
                    "name", "id" -> name += " " + pair.second.lowercase()
                }
            }
            if (type == "password" || ac.contains("password")) return Kind.PASSWORD
            if (type == "email" || ac.contains("username") || ac.contains("email") ||
                name.contains("user") || name.contains("email") || name.contains("login")
            ) {
                return Kind.USERNAME
            }
        }
        if (isPasswordField(node)) return Kind.PASSWORD
        return Kind.NONE
    }

    private fun findDomain(node: AssistStructure.ViewNode): String? {
        node.webDomain?.let { if (it.isNotEmpty()) return it }
        for (i in 0 until node.childCount) {
            findDomain(node.getChildAt(i))?.let { return it }
        }
        return null
    }

    private fun isPasswordField(node: AssistStructure.ViewNode): Boolean {
        val t = node.inputType
        val variation = t and InputType.TYPE_MASK_VARIATION
        val cls = t and InputType.TYPE_MASK_CLASS
        return cls == InputType.TYPE_CLASS_TEXT &&
            (variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD)
    }

    private fun traverse(node: AssistStructure.ViewNode, visit: (AssistStructure.ViewNode) -> Unit) {
        visit(node)
        for (i in 0 until node.childCount) {
            traverse(node.getChildAt(i), visit)
        }
    }

    // --- список сайтов ---

    private fun readSites(): Sites? {
        // sites.txt пишет Rust через app_data_dir; на Android это не обязательно
        // filesDir, поэтому ищем по всему каталогу данных приложения
        val root = filesDir.parentFile ?: filesDir
        val file = findSitesFile(root, 0) ?: return null
        var m = 20
        var t = 21
        val lines = ArrayList<SiteLine>()
        for (raw in file.readLines()) {
            val line = raw.trim()
            if (line.isEmpty()) continue
            if (line.startsWith("#")) {
                val toks = line.removePrefix("#").trim().split(Regex("\\s+"))
                if (toks.size == 3 && toks[0] == "kdf") {
                    toks[1].removePrefix("M").toIntOrNull()?.let { m = it }
                    toks[2].removePrefix("T").toIntOrNull()?.let { t = it }
                }
                continue
            }
            val toks = line.split(Regex("\\s+"))
            val name = toks.firstOrNull() ?: continue
            var login = ""
            var aliases = emptyList<String>()
            var label = ""
            for (tok in toks.drop(1)) {
                when {
                    tok.startsWith("login=") -> login = tok.removePrefix("login=")
                    tok.startsWith("alias=") ->
                        aliases = tok.removePrefix("alias=").split(',').filter { it.isNotEmpty() }
                    // пробелы в label закодированы как %20 (см. store.rs)
                    tok.startsWith("label=") ->
                        label = tok.removePrefix("label=").replace("%20", " ").replace("%25", "%")
                }
            }
            lines.add(SiteLine(name, login, aliases, label, line))
        }
        return Sites(m, t, lines)
    }

    // Обход каталога данных приложения в поисках sites.txt. Пропускаем заведомо
    // тяжёлые/ненужные подкаталоги (кэш, данные webview), ограничиваем глубину.
    private fun findSitesFile(dir: File, depth: Int): File? {
        if (depth > 5) return null
        val direct = File(dir, "sites.txt")
        if (direct.isFile) return direct
        val children = dir.listFiles() ?: return null
        for (c in children) {
            if (c.isFile) {
                if (c.name == "sites.txt") return c
            } else if (c.isDirectory && c.name !in SKIP_DIRS) {
                findSitesFile(c, depth + 1)?.let { return it }
            }
        }
        return null
    }
}

private val SKIP_DIRS = setOf("cache", "code_cache", "app_webview", "app_textures")
