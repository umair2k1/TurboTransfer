package com.turbotransfer

import android.app.Application
import dagger.hilt.android.HiltAndroidApp

@HiltAndroidApp
class TurboTransferApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        try {
            System.loadLibrary("turbotransfer_core")
            uniffi.turbotransfer_core.initLogger()
            val appDataDir = filesDir.absolutePath
            uniffi.turbotransfer_core.setDataDirectory(appDataDir)
            try {
                val setTempDirMethod =
                        Class.forName("uniffi.turbotransfer_core.Turbotransfer_coreKt")
                                .getMethod("setTempDirectory", String::class.java)
                setTempDirMethod.invoke(null, cacheDir.absolutePath)
            } catch (_: Throwable) {}
        } catch (e: UnsatisfiedLinkError) {
            e.printStackTrace()
        } catch (e: Throwable) {
            e.printStackTrace()
        }
    }
}
