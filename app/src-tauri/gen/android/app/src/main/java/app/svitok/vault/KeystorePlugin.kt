package app.svitok.vault

import android.app.Activity
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.view.WindowManager
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

// 16 байт сида лежат зашифрованными: AES-256-GCM, ключ в Android Keystore.
// Ключ требует аутентификацию (отпечаток или PIN устройства) и не выгружается из TEE.
// Каждый раз, когда сид нужен (создание, разблокировка), идём через BiometricPrompt
// с CryptoObject. Без подтверждения личности на этом устройстве расшифровки не будет.

@InvokeArg
internal class StoreArgs {
    lateinit var seed: String // hex, 32 символа
}

@InvokeArg
internal class SecureArgs {
    var on: Boolean = true
}

@InvokeArg
internal class ClipArgs {
    lateinit var text: String
}

@InvokeArg
internal class ScanArgs {
    var hint: String = ""
}

private const val KEY_ALIAS = "svitok_seed_key_v2"
private const val FILE_NAME = "seed.enc"
private const val GCM_TAG_BITS = 128

@TauriPlugin
class KeystorePlugin(private val activity: Activity) : Plugin(activity) {

    private val keyStore: KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private fun seedFile() = File(activity.filesDir, FILE_NAME)

    private fun getOrCreateKey(): SecretKey {
        (keyStore.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val builder = KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(true)
            // новый отпечаток не убивает ключ. даже если бы убил - сид есть на бумаге
            .setInvalidatedByBiometricEnrollment(false)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            builder.setUserAuthenticationParameters(
                0, // ноль - спрашивать при каждом использовании
                // только биометрия. PIN/паттерн устройства не пускаем: сид не должен
                // открываться знанием кода блокировки (его легко подсмотреть)
                KeyProperties.AUTH_BIOMETRIC_STRONG,
            )
        } else {
            @Suppress("DEPRECATION")
            builder.setUserAuthenticationValidityDurationSeconds(-1)
        }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(builder.build())
        return gen.generateKey()
    }

    private fun hexToBytes(s: String) = ByteArray(s.length / 2) {
        ((Character.digit(s[it * 2], 16) shl 4) + Character.digit(s[it * 2 + 1], 16)).toByte()
    }

    private fun bytesToHex(b: ByteArray): String {
        val sb = StringBuilder(b.size * 2)
        for (x in b) sb.append("%02x".format(x))
        return sb.toString()
    }

    /// Показать BiometricPrompt для cipher, при успехе передать готовый cipher в onCipher.
    private fun authAndRun(cipher: Cipher, onCipher: (Cipher) -> Unit, onError: (String) -> Unit) {
        activity.runOnUiThread {
            val fa = activity as? FragmentActivity
            if (fa == null) {
                onError("активность не FragmentActivity")
                return@runOnUiThread
            }
            // только биометрия на всех версиях: сид не открываем кодом блокировки
            val allowed = BiometricManager.Authenticators.BIOMETRIC_STRONG
            val prompt = BiometricPrompt(
                fa,
                ContextCompat.getMainExecutor(activity),
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                        val c = result.cryptoObject?.cipher
                        if (c == null) {
                            onError("нет cipher после аутентификации")
                            return
                        }
                        try {
                            onCipher(c)
                        } catch (e: Exception) {
                            onError("крипто: ${e.message}")
                        }
                    }

                    override fun onAuthenticationError(code: Int, msg: CharSequence) {
                        onError("аутентификация отменена ($code)")
                    }
                },
            )
            val builder = BiometricPrompt.PromptInfo.Builder()
                .setTitle("Свиток")
                .setSubtitle("Подтвердите личность")
                .setAllowedAuthenticators(allowed)
            // разрешена только биометрия - значит кнопка отмены обязательна
            builder.setNegativeButtonText("Отмена")
            try {
                prompt.authenticate(builder.build(), BiometricPrompt.CryptoObject(cipher))
            } catch (e: Exception) {
                onError("prompt: ${e.message}")
            }
        }
    }

    @Command
    fun setSecure(invoke: Invoke) {
        val args = invoke.parseArgs(SecureArgs::class.java)
        activity.runOnUiThread {
            if (args.on) {
                activity.window.setFlags(
                    WindowManager.LayoutParams.FLAG_SECURE,
                    WindowManager.LayoutParams.FLAG_SECURE,
                )
            } else {
                activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
            }
        }
        invoke.resolve()
    }

    @Command
    fun hasSeed(invoke: Invoke) {
        val r = JSObject()
        r.put("value", seedFile().exists())
        invoke.resolve(r)
    }

    @Command
    fun storeSeed(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(StoreArgs::class.java)
            val seed = hexToBytes(args.seed)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey())
            authAndRun(cipher, { c ->
                val ct = c.doFinal(seed)
                val iv = c.iv
                val out = ByteArray(1 + iv.size + ct.size)
                out[0] = iv.size.toByte()
                System.arraycopy(iv, 0, out, 1, iv.size)
                System.arraycopy(ct, 0, out, 1 + iv.size, ct.size)
                seedFile().writeBytes(out)
                seed.fill(0)
                invoke.resolve()
            }, { err ->
                seed.fill(0)
                invoke.reject(err)
            })
        } catch (e: Exception) {
            invoke.reject("keystore store: ${e.message}")
        }
    }

    @Command
    fun loadSeed(invoke: Invoke) {
        try {
            val blob = seedFile().readBytes()
            if (blob.size < 2) {
                invoke.reject("сид повреждён")
                return
            }
            val ivLen = blob[0].toInt()
            if (ivLen <= 0 || blob.size < 1 + ivLen + 16) {
                invoke.reject("сид повреждён")
                return
            }
            val iv = blob.copyOfRange(1, 1 + ivLen)
            val ct = blob.copyOfRange(1 + ivLen, blob.size)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, getOrCreateKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            authAndRun(cipher, { c ->
                val seed = c.doFinal(ct)
                val r = JSObject()
                r.put("seed", bytesToHex(seed))
                seed.fill(0)
                invoke.resolve(r)
            }, { err ->
                invoke.reject(err)
            })
        } catch (e: Exception) {
            invoke.reject("keystore load: ${e.message}")
        }
    }

    @Command
    fun clearSeed(invoke: Invoke) {
        seedFile().delete()
        try {
            keyStore.deleteEntry(KEY_ALIAS)
        } catch (_: Exception) {
        }
        invoke.resolve()
    }

    // Копия пароля с пометкой «чувствительно»: на Android 13+ система прячет её
    // из превью буфера и не гонит в облачную синхронизацию клавиатур.
    @Command
    fun copyClip(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ClipArgs::class.java)
            val cm = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val clip = ClipData.newPlainText("svitok", args.text)
            if (Build.VERSION.SDK_INT >= 33) {
                clip.description.extras = PersistableBundle().apply {
                    putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
                }
            }
            cm.setPrimaryClip(clip)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("clipboard: ${e.message}")
        }
    }

    // Свой сканер QR (ScannerActivity, CameraX+ZXing). Ответ отдаёт активность
    // через ScannerActivity.deliver - resolve с текстом кода или reject.
    @Command
    fun scanQr(invoke: Invoke) {
        val args = invoke.parseArgs(ScanArgs::class.java)
        ScannerActivity.pending?.reject("scan-restarted")
        ScannerActivity.pending = invoke
        val intent = android.content.Intent(activity, ScannerActivity::class.java)
        intent.putExtra(ScannerActivity.EXTRA_HINT, args.hint)
        activity.startActivity(intent)
    }

    @Command
    fun clearClip(invoke: Invoke) {
        try {
            val cm = activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            if (Build.VERSION.SDK_INT >= 28) {
                cm.clearPrimaryClip()
            } else {
                cm.setPrimaryClip(ClipData.newPlainText("", ""))
            }
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("clipboard: ${e.message}")
        }
    }
}
