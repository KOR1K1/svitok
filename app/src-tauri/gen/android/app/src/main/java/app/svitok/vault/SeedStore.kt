package app.svitok.vault

import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec

// Чтение сида для компонентов вне Tauri (AutofillService). Тот же ключ Keystore и
// тот же формат файла, что у KeystorePlugin: [ivLen][iv][ciphertext], AES-256-GCM.
// Ключ здесь только достаётся и используется - не создаётся: сид уже есть, раз
// пользователь завёл Свиток. Расшифровка идёт под биометрией через CryptoObject.
object SeedStore {
    private const val KEY_ALIAS = "svitok_seed_key_v2"
    private const val FILE_NAME = "seed.enc"
    private const val GCM_TAG_BITS = 128

    private val keyStore: KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    fun hasSeed(activity: FragmentActivity): Boolean = File(activity.filesDir, FILE_NAME).exists()

    /// Запрашивает биометрию и при успехе отдаёт сид в hex. onHex вызывается в UI-потоке.
    fun unlockSeedHex(
        activity: FragmentActivity,
        onHex: (String) -> Unit,
        onError: (String) -> Unit,
    ) {
        try {
            val blob = File(activity.filesDir, FILE_NAME).readBytes()
            if (blob.size < 2) {
                onError("сид повреждён")
                return
            }
            val ivLen = blob[0].toInt()
            if (ivLen <= 0 || blob.size < 1 + ivLen + 16) {
                onError("сид повреждён")
                return
            }
            val iv = blob.copyOfRange(1, 1 + ivLen)
            val ct = blob.copyOfRange(1 + ivLen, blob.size)
            val key = (keyStore.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.secretKey
                ?: run { onError("ключ сида не найден"); return }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(GCM_TAG_BITS, iv))
            authAndRun(activity, cipher, { c ->
                val seed = c.doFinal(ct)
                val hex = bytesToHex(seed)
                seed.fill(0)
                onHex(hex)
            }, onError)
        } catch (e: Exception) {
            onError("сид: ${e.message}")
        }
    }

    private fun authAndRun(
        activity: FragmentActivity,
        cipher: Cipher,
        onCipher: (Cipher) -> Unit,
        onError: (String) -> Unit,
    ) {
        activity.runOnUiThread {
            val prompt = BiometricPrompt(
                activity,
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
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle("Свиток")
                .setSubtitle("Автозаполнение пароля")
                .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
                .setNegativeButtonText("Отмена")
                .build()
            try {
                prompt.authenticate(info, BiometricPrompt.CryptoObject(cipher))
            } catch (e: Exception) {
                onError("prompt: ${e.message}")
            }
        }
    }

    private fun bytesToHex(b: ByteArray): String {
        val sb = StringBuilder(b.size * 2)
        for (x in b) sb.append("%02x".format(x))
        return sb.toString()
    }
}
