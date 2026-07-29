package org.onebrain.onebrain_mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val ANDROID_KEY_STORE = "AndroidKeyStore"
private const val WRAPPING_KEY_ALIAS = "onebrain.mobile.install-wrapping.v1"
private const val SECURITY_MATERIAL_BYTES = 192
private const val GCM_TAG_BITS = 128
private const val ENVELOPE_VERSION: Byte = 1
private val MARKER_MAGIC = "OBMARK01".toByteArray(Charsets.US_ASCII)
private val MARKER_CONTEXT = "onebrain:mobile:install-marker:1\u0000".toByteArray(Charsets.UTF_8)

internal class SecurityMaterialStore(context: Context) {
    private val root = context.noBackupFilesDir
    private val securityDirectory = File(root, "security")
    private val envelopeFile = File(securityDirectory, "root-material.v1")
    private val markerFile = File(securityDirectory, "install-marker.v1")
    private val keyStore = KeyStore.getInstance(ANDROID_KEY_STORE).apply { load(null) }

    @Synchronized
    fun loadOrCreate(): ByteArray {
        val markerExists = markerFile.isFile
        val envelopeExists = envelopeFile.isFile
        if (!markerExists && !envelopeExists) {
            checkNoUnboundAuthority()
            retireOrphanedWrappingKey()
            return createInstallation()
        }
        check(markerExists && envelopeExists) {
            "UNEXPECTED_RESTORE: install marker and protected envelope must exist together"
        }
        check(keyStore.containsAlias(WRAPPING_KEY_ALIAS)) {
            "UNEXPECTED_RESTORE: protected envelope has no nonportable wrapping key"
        }
        val material = decryptEnvelope(envelopeFile.readBytes(), wrappingKey())
        try {
            check(material.size == SECURITY_MATERIAL_BYTES) {
                "SECURITY_MATERIAL_INVALID: unexpected protected material length"
            }
            check(MessageDigest.isEqual(markerFile.readBytes(), marker(material))) {
                "UNEXPECTED_RESTORE: install marker does not bind the protected material"
            }
            return material
        } catch (error: Throwable) {
            material.fill(0)
            throw error
        }
    }

    private fun checkNoUnboundAuthority() {
        check(!File(root, "bootstrap.redb").exists()) {
            "UNEXPECTED_RESTORE: authority bytes exist without an install marker"
        }
        check(!File(root, "private-vault.redb").exists()) {
            "UNEXPECTED_RESTORE: private vault exists without an install marker"
        }
        check(!File(root, "private-drafts.redb").exists()) {
            "UNEXPECTED_RESTORE: private drafts exist without an install marker"
        }
        check(!File(root, "private-media-staging.redb").exists()) {
            "UNEXPECTED_RESTORE: media staging metadata exists without an install marker"
        }
        check(!File(root, "media").exists()) {
            "UNEXPECTED_RESTORE: encrypted media bytes exist without an install marker"
        }
    }

    private fun retireOrphanedWrappingKey() {
        if (keyStore.containsAlias(WRAPPING_KEY_ALIAS)) {
            keyStore.deleteEntry(WRAPPING_KEY_ALIAS)
        }
    }

    private fun createInstallation(): ByteArray {
        check(securityDirectory.mkdirs() || securityDirectory.isDirectory) {
            "SECURITY_STORAGE_UNAVAILABLE: cannot create no-backup security directory"
        }
        val material = ByteArray(SECURITY_MATERIAL_BYTES)
        SecureRandom().nextBytes(material)
        try {
            val key = createWrappingKey()
            atomicWrite(envelopeFile, encryptEnvelope(material, key))
            atomicWrite(markerFile, marker(material))
            return material
        } catch (error: Throwable) {
            material.fill(0)
            envelopeFile.delete()
            markerFile.delete()
            keyStore.deleteEntry(WRAPPING_KEY_ALIAS)
            throw error
        }
    }

    private fun createWrappingKey(): SecretKey {
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEY_STORE)
        val specification =
            KeyGenParameterSpec
                .Builder(
                    WRAPPING_KEY_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true)
                .build()
        generator.init(specification)
        return generator.generateKey()
    }

    private fun wrappingKey(): SecretKey =
        keyStore.getKey(WRAPPING_KEY_ALIAS, null) as? SecretKey
            ?: error("SECURITY_KEY_UNAVAILABLE: wrapping key has an unexpected type")

    private fun encryptEnvelope(
        material: ByteArray,
        key: SecretKey,
    ): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        cipher.updateAAD(MARKER_CONTEXT)
        val ciphertext = cipher.doFinal(material)
        return ByteBuffer
            .allocate(2 + cipher.iv.size + ciphertext.size)
            .put(ENVELOPE_VERSION)
            .put(cipher.iv.size.toByte())
            .put(cipher.iv)
            .put(ciphertext)
            .array()
    }

    private fun decryptEnvelope(
        envelope: ByteArray,
        key: SecretKey,
    ): ByteArray {
        check(envelope.size >= 2 + 12 + 16) {
            "SECURITY_ENVELOPE_CORRUPT: protected envelope is truncated"
        }
        val input = ByteBuffer.wrap(envelope)
        check(input.get() == ENVELOPE_VERSION) {
            "SECURITY_ENVELOPE_UNSUPPORTED: protected envelope version is not supported"
        }
        val ivLength = input.get().toInt() and 0xff
        check(ivLength in 12..32 && input.remaining() > ivLength + 16) {
            "SECURITY_ENVELOPE_CORRUPT: invalid GCM IV or ciphertext"
        }
        val iv = ByteArray(ivLength)
        input.get(iv)
        val ciphertext = ByteArray(input.remaining())
        input.get(ciphertext)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(GCM_TAG_BITS, iv))
        cipher.updateAAD(MARKER_CONTEXT)
        return cipher.doFinal(ciphertext)
    }

    private fun marker(material: ByteArray): ByteArray {
        val digest = MessageDigest.getInstance("SHA-256")
        digest.update(MARKER_CONTEXT)
        digest.update(material)
        return MARKER_MAGIC + digest.digest()
    }

    private fun atomicWrite(
        target: File,
        bytes: ByteArray,
    ) {
        val temporary = File(target.parentFile, "${target.name}.creating")
        FileOutputStream(temporary).use { output ->
            output.write(bytes)
            output.fd.sync()
        }
        check(!target.exists()) {
            "SECURITY_STORAGE_CONFLICT: refusing to overwrite protected installation state"
        }
        check(temporary.renameTo(target)) {
            "SECURITY_STORAGE_UNAVAILABLE: cannot atomically install ${target.name}"
        }
    }
}
