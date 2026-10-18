package com.turbotransfer.data.source.local

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import android.os.Environment
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SettingsLocalDataSource @Inject constructor(
    @ApplicationContext private val context: Context
) {
    private val prefsName = "turbotransfer_settings"
    private val keyDeviceName = "device_name"
    private val keyPrefer5Ghz = "prefer_5ghz"
    private val keyAutoWakeLock = "auto_wake_lock"
    private val keyDestDir = "dest_dir"
    private val keyAutoAcceptTransfers = "auto_accept_transfers"
    private val keyManageStorageDontAskAgain = "manage_storage_dont_ask_again"

    private val prefs: SharedPreferences by lazy {
        context.getSharedPreferences(prefsName, Context.MODE_PRIVATE)
    }

    fun getDeviceName(): String {
        return prefs.getString(keyDeviceName, Build.MODEL) ?: Build.MODEL
    }

    fun setDeviceName(name: String) {
        prefs.edit().putString(keyDeviceName, name).apply()
    }

    fun is5GhzPreferred(): Boolean {
        return prefs.getBoolean(keyPrefer5Ghz, true)
    }

    fun set5GhzPreferred(enabled: Boolean) {
        prefs.edit().putBoolean(keyPrefer5Ghz, enabled).apply()
    }

    fun isAutoWakeLockEnabled(): Boolean {
        return prefs.getBoolean(keyAutoWakeLock, true)
    }

    fun setAutoWakeLockEnabled(enabled: Boolean) {
        prefs.edit().putBoolean(keyAutoWakeLock, enabled).apply()
    }

    fun getReceiveDestDir(): String {
        return prefs.getString(
            keyDestDir,
            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
        ) ?: Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
    }

    fun setReceiveDestDir(dir: String) {
        prefs.edit().putString(keyDestDir, dir).apply()
    }

    fun isAutoAcceptTransfers(): Boolean {
        return prefs.getBoolean(keyAutoAcceptTransfers, false)
    }

    fun setAutoAcceptTransfers(enabled: Boolean) {
        prefs.edit().putBoolean(keyAutoAcceptTransfers, enabled).apply()
    }

    fun isManageStorageDontAskAgain(): Boolean {
        return prefs.getBoolean(keyManageStorageDontAskAgain, false)
    }

    fun setManageStorageDontAskAgain(dontAsk: Boolean) {
        prefs.edit().putBoolean(keyManageStorageDontAskAgain, dontAsk).apply()
    }
}
