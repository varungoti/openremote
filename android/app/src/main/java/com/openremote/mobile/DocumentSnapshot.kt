package com.openremote.mobile

import android.content.Context
import android.net.Uri
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.ensureActive
import java.io.File
import java.util.concurrent.atomic.AtomicReference

private val cacheRecovery = UploadCacheRecovery()
internal fun prepareUploadCache(context: Context) = cacheRecovery.prepare(File(context.cacheDir, "outgoing-files"))

/** Only a user-selected SAF content URI is read; no storage permission or persistent URI grant. */
internal suspend fun snapshotDocument(context: Context, uri: Uri): UploadSnapshot = coroutineScope {
    require(uri.scheme == "content") { "Choose a file from Android's document picker" }
    val signal = CancellationSignal()
    val descriptor = AtomicReference<ParcelFileDescriptor?>(null)
    val snapshot = AtomicReference<UploadSnapshot?>(null)
    var delivered = false
    val task = async(Dispatchers.IO) {
        val resolver = context.contentResolver
        var name: String? = null
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, signal)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (nameIndex >= 0 && !cursor.isNull(nameIndex)) name = cursor.getString(nameIndex)
                val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
                if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) require(cursor.getLong(sizeIndex) <= ToolProtocol.MAX_FILE_BYTES) { "Files must be at most 64 MiB" }
            }
        }
        val displayName = requireNotNull(name) { "The selected document has no filename" }
        require(ToolProtocol.validName(displayName)) { "This filename is not supported on Windows. Rename it before sending." }
        val opened = resolver.openFileDescriptor(uri, "r", signal) ?: error("The selected document cannot be opened")
        try {
            descriptor.set(opened)
            ParcelFileDescriptor.AutoCloseInputStream(opened).use { input ->
                ensureActive()
                UploadSnapshot.create(prepareUploadCache(context), displayName, input).also { snapshot.set(it); ensureActive() }
            }
        } catch (e: Exception) { snapshot.getAndSet(null)?.close(); throw e
        } finally { descriptor.compareAndSet(opened, null); runCatching { opened.close() } }
    }
    try { task.await().also { delivered = true } }
    finally {
        signal.cancel()
        runCatching { descriptor.getAndSet(null)?.close() }
        task.cancel()
        if (!delivered) snapshot.getAndSet(null)?.close()
    }
}
