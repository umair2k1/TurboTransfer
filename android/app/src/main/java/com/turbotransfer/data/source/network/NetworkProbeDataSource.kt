package com.turbotransfer.data.source.network

import android.content.Context
import android.net.wifi.WifiManager
import com.turbotransfer.core.common.DispatcherProvider
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.Inet4Address
import java.net.InetSocketAddress
import java.net.NetworkInterface
import java.net.Socket
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class NetworkProbeDataSource @Inject constructor(
    @ApplicationContext private val context: Context,
    private val dispatcherProvider: DispatcherProvider
) {
    suspend fun probeUsbTunnel(port: Int = 9876, timeoutMs: Int = 250): Boolean = withContext(dispatcherProvider.io) {
        try {
            Socket().use { socket ->
                socket.connect(InetSocketAddress("127.0.0.1", port), timeoutMs)
                true
            }
        } catch (_: Exception) {
            false
        }
    }

    suspend fun probeCandidateWifiReceivers(port: Int = 9876): String? = withContext(dispatcherProvider.io) {
        val candidateIps = mutableSetOf(
            "10.18.163.1",
            "10.18.163.2",
            "192.168.43.1",
            "192.168.43.2",
            "192.168.137.1"
        )

        // 1. Check Wi-Fi DHCP gateway / server
        try {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            val dhcp = wifiManager?.dhcpInfo
            if (dhcp != null) {
                if (dhcp.gateway != 0) {
                    val gw = intToIp(dhcp.gateway)
                    if (gw.isNotBlank()) candidateIps.add(gw)
                }
                if (dhcp.serverAddress != 0) {
                    val srv = intToIp(dhcp.serverAddress)
                    if (srv.isNotBlank()) candidateIps.add(srv)
                }
            }
        } catch (_: Exception) {}

        // 2. Add subnet candidates from local active Wi-Fi interfaces (wlan*, p2p*, ap*, softap*, etc.)
        val wifiIps = getLocalWifiIpAddresses()
        for (localIp in wifiIps) {
            val parts = localIp.split(".")
            if (parts.size == 4) {
                val prefix = "${parts[0]}.${parts[1]}.${parts[2]}"
                val localHost = parts[3].toIntOrNull() ?: 1

                // 2a. High priority: Gateways, common APs
                candidateIps.add("$prefix.1")
                candidateIps.add("$prefix.2")
                candidateIps.add("$prefix.254")

                // 2b. High priority: Common DHCP ranges & hotspot allocations
                for (h in listOf(50, 51, 52, 53, 54, 55, 100, 101, 102, 103, 104, 105, 150, 200, 225, 238, 19, 20, 21, 22)) {
                    if (h != localHost) candidateIps.add("$prefix.$h")
                }

                // 2c. Close neighbors near local host (±20)
                for (offset in 1..20) {
                    val h1 = localHost + offset
                    val h2 = localHost - offset
                    if (h1 in 1..254 && h1 != localHost) candidateIps.add("$prefix.$h1")
                    if (h2 in 1..254 && h2 != localHost) candidateIps.add("$prefix.$h2")
                }

                // 2d. Complete /24 subnet sweep (1..254)
                for (host in 1..254) {
                    if (host != localHost) {
                        candidateIps.add("$prefix.$host")
                    }
                }
            }
        }

        // 3. Scan ARP table entries if accessible (legacy Android / fallback)
        try {
            val arpLines = File("/proc/net/arp").readLines()
            for (line in arpLines.drop(1)) {
                val tokens = line.trim().split(Regex("\\s+"))
                if (tokens.size >= 6) {
                    val ip = tokens[0]
                    val dev = tokens[5].lowercase()
                    if (ip.matches(Regex("\\d+\\.\\d+\\.\\d+\\.\\d+")) && (dev.startsWith("wlan") || dev.startsWith("p2p") || dev.startsWith("ap") || dev.startsWith("swlan"))) {
                        candidateIps.add(ip)
                    }
                }
            }
        } catch (_: Exception) {}

        probeIpSet(candidateIps, port)
    }

    suspend fun probeCandidateUsbTetherReceivers(port: Int = 9876): String? = withContext(dispatcherProvider.io) {
        val candidateIps = mutableSetOf<String>()

        // 1. Add subnet candidates from USB tether / RNDIS interfaces (rndis*, usb*, ncm*)
        val tetherIps = getLocalUsbTetherIpAddresses()
        for (localIp in tetherIps) {
            val parts = localIp.split(".")
            if (parts.size == 4) {
                val prefix = "${parts[0]}.${parts[1]}.${parts[2]}"
                val localHost = parts[3].toIntOrNull() ?: 1

                candidateIps.add("$prefix.1")
                candidateIps.add("$prefix.2")
                candidateIps.add("$prefix.30")
                candidateIps.add("$prefix.65")
                candidateIps.add("$prefix.100")
                candidateIps.add("$prefix.254")

                for (offset in 1..20) {
                    val h1 = localHost + offset
                    val h2 = localHost - offset
                    if (h1 in 1..254 && h1 != localHost) candidateIps.add("$prefix.$h1")
                    if (h2 in 1..254 && h2 != localHost) candidateIps.add("$prefix.$h2")
                }

                for (host in 1..254) {
                    if (host != localHost) {
                        candidateIps.add("$prefix.$host")
                    }
                }
            }
        }

        // 2. Scan ARP table entries for resolved neighbors on rndis/usb devices
        try {
            val arpLines = File("/proc/net/arp").readLines()
            for (line in arpLines.drop(1)) {
                val tokens = line.trim().split(Regex("\\s+"))
                if (tokens.size >= 6) {
                    val ip = tokens[0]
                    val dev = tokens[5].lowercase()
                    if (ip.matches(Regex("\\d+\\.\\d+\\.\\d+\\.\\d+")) && (dev.startsWith("rndis") || dev.startsWith("usb") || dev.startsWith("ncm"))) {
                        candidateIps.add(ip)
                    }
                }
            }
        } catch (_: Exception) {}

        probeIpSet(candidateIps, port)
    }

    private suspend fun probeIpSet(candidateIps: Collection<String>, port: Int, timeoutMs: Int = 180): String? = withContext(dispatcherProvider.io) {
        if (candidateIps.isEmpty()) return@withContext null
        val result = CompletableDeferred<String>()
        val parentJob = Job()
        val scope = CoroutineScope(coroutineContext + parentJob)
        val remaining = java.util.concurrent.atomic.AtomicInteger(candidateIps.size)

        for (targetIp in candidateIps) {
            scope.launch(dispatcherProvider.io) {
                try {
                    Socket().use { socket ->
                        socket.connect(InetSocketAddress(targetIp, port), timeoutMs)
                        if (result.complete(targetIp)) {
                            parentJob.cancel()
                        }
                    }
                } catch (_: Exception) {
                    if (remaining.decrementAndGet() == 0) {
                        result.completeExceptionally(NoSuchElementException())
                    }
                }
            }
        }

        try {
            result.await()
        } catch (_: Exception) {
            null
        } finally {
            parentJob.cancel()
        }
    }

    private fun intToIp(i: Int): String {
        return "${i and 0xFF}.${i shr 8 and 0xFF}.${i shr 16 and 0xFF}.${i shr 24 and 0xFF}"
    }

    suspend fun getLocalWifiIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        getFilteredIpAddresses { name ->
            val lower = name.lowercase()
            lower.startsWith("wlan") || lower.startsWith("p2p") || lower.startsWith("ap") || lower.startsWith("softap") || lower.startsWith("swlan") || lower.startsWith("wigig") || lower.contains("wifi")
        }
    }

    suspend fun getLocalUsbTetherIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        getFilteredIpAddresses { name ->
            val lower = name.lowercase()
            lower.startsWith("rndis") || lower.startsWith("usb") || lower.startsWith("ncm")
        }
    }

    suspend fun getLocalIpAddresses(): List<String> = withContext(dispatcherProvider.io) {
        getFilteredIpAddresses { true }
    }

    private suspend fun getFilteredIpAddresses(filter: (String) -> Boolean): List<String> = withContext(dispatcherProvider.io) {
        try {
            NetworkInterface.getNetworkInterfaces()?.asSequence().orEmpty()
                .filter { !it.isLoopback && it.isUp && filter(it.name) }
                .flatMap { it.inetAddresses.asSequence() }
                .filterIsInstance<Inet4Address>()
                .mapNotNull { it.hostAddress }
                .filter { it.isNotBlank() && !it.startsWith("127.") }
                .distinct()
                .toList()
        } catch (_: Exception) {
            emptyList()
        }
    }
}
