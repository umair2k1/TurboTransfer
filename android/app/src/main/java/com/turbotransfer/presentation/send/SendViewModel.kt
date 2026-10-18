package com.turbotransfer.presentation.send

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.turbotransfer.core.common.Resource
import com.turbotransfer.data.repository.DiscoveryRepositoryImpl
import com.turbotransfer.data.repository.HotspotRepositoryImpl
import com.turbotransfer.data.repository.TransferRepositoryImpl
import com.turbotransfer.domain.model.SelectedFileInfo
import com.turbotransfer.domain.model.TransferStatus
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch

@HiltViewModel
class SendViewModel
@Inject
constructor(
        private val discoveryRepository: DiscoveryRepositoryImpl,
        private val hotspotRepository: HotspotRepositoryImpl,
        private val transferRepository: TransferRepositoryImpl
) : ViewModel() {

    private val _uiState = MutableStateFlow(SendUiState())
    val uiState: StateFlow<SendUiState> = _uiState.asStateFlow()

    private var batchTransferJob: Job? = null

    init {
        // Observe receiver discovery
        viewModelScope.launch {
            discoveryRepository.observeReceiverDiscovery().collect { receiver ->
                _uiState.update { current ->
                    val updatedAddress =
                            if (receiver != null &&
                                            (current.customAddress == "127.0.0.1:9876" ||
                                                    current.customAddress.isEmpty() ||
                                                    current.customAddress.contains("127.0.0.1"))
                            ) {
                                receiver.address
                            } else {
                                current.customAddress
                            }
                    current.copy(discoveredReceiver = receiver, customAddress = updatedAddress)
                }
            }
        }

        // Observe hotspot state
        viewModelScope.launch {
            hotspotRepository.hotspotStateFlow.collect { hotspotState ->
                _uiState.update { it.copy(hotspotState = hotspotState) }
            }
        }
    }

    fun addFilesToQueue(files: List<SelectedFileInfo>) {
        _uiState.update { current ->
            val updated = current.transferQueue.toMutableList()
            for (f in files) {
                if (updated.none { it.path == f.path }) {
                    updated.add(f)
                }
            }
            current.copy(transferQueue = updated)
        }
    }

    fun removeFileFromQueue(file: SelectedFileInfo) {
        _uiState.update { current ->
            val updated = current.transferQueue.toMutableList()
            updated.remove(file)
            current.copy(transferQueue = updated)
        }
    }

    fun clearQueue() {
        batchTransferJob?.cancel()
        _uiState.update {
            it.copy(transferQueue = emptyList(), isQueueRunning = false, currentQueueIndex = 0)
        }
    }

    fun setCustomAddress(address: String) {
        _uiState.update { it.copy(customAddress = address) }
    }

    fun toggleCustomAddressField() {
        _uiState.update { it.copy(showCustomAddressField = !it.showCustomAddressField) }
    }

    fun setShowQrDialog(show: Boolean) {
        _uiState.update { it.copy(showQrDialog = show) }
    }

    fun toggleHotspot() {
        val active = _uiState.value.hotspotState.isActive
        if (!active) {
            hotspotRepository.startHotspot(9876) { res ->
                when (res) {
                    is Resource.Success -> _uiState.update { it.copy(userMessage = res.data) }
                    is Resource.Error -> _uiState.update { it.copy(userMessage = res.message) }
                    is Resource.Loading -> {}
                }
            }
        } else {
            hotspotRepository.stopHotspot()
        }
    }

    fun startBatchTransfer(targetAddress: String, onTransferStarted: () -> Unit) {
        val queue = _uiState.value.transferQueue
        if (queue.isEmpty()) {
            _uiState.update { it.copy(userMessage = "Please select at least one file to send") }
            return
        }

        batchTransferJob?.cancel()
        batchTransferJob =
                viewModelScope.launch {
                    val fullBatch = queue.toList()
                    transferRepository.setBatchQueue(fullBatch)
                    _uiState.update { it.copy(isQueueRunning = true, currentQueueIndex = 0) }
                    onTransferStarted()

                    while (_uiState.value.transferQueue.isNotEmpty()) {
                        val item = _uiState.value.transferQueue.first()
                        val currentIndex = fullBatch.indexOfFirst { it.path == item.path }.coerceAtLeast(0)
                        _uiState.update { it.copy(currentQueueIndex = currentIndex) }
                        transferRepository.updateCurrentQueueIndex(currentIndex)
                        val res =
                                transferRepository.startTransfer(
                                        item.path,
                                        targetAddress,
                                        item.displayName
                                )
                        when (res) {
                            is Resource.Success -> {
                                val transferId = res.data
                                // Await terminal status for this specific item before proceeding to
                                // the next
                                val terminalProgress =
                                        transferRepository.observeTransferProgress(transferId)
                                                .firstOrNull { progress ->
                                                    progress != null &&
                                                            (progress.status ==
                                                                    TransferStatus.COMPLETED ||
                                                                    progress.status ==
                                                                            TransferStatus.FAILED ||
                                                                    progress.status ==
                                                                            TransferStatus
                                                                                    .CANCELLED)
                                                }

                                if (terminalProgress == null ||
                                                terminalProgress.status != TransferStatus.COMPLETED
                                ) {
                                    _uiState.update {
                                        it.copy(
                                                isQueueRunning = false,
                                                userMessage =
                                                        "Transfer stopped or failed for ${item.displayName}"
                                        )
                                    }
                                    return@launch
                                }

                                removeFileFromQueue(item)
                                // If stopped or cancelled, exit loop
                                if (!_uiState.value.isQueueRunning) {
                                    return@launch
                                }
                            }
                            is Resource.Error -> {
                                _uiState.update {
                                    it.copy(
                                            userMessage =
                                                    "Error sending ${item.displayName}: ${res.message}",
                                            isQueueRunning = false
                                    )
                                }
                                return@launch
                            }
                            is Resource.Loading -> {}
                        }
                    }

                    _uiState.update { it.copy(isQueueRunning = false, currentQueueIndex = 0) }
                }
    }

    fun clearUserMessage() {
        _uiState.update { it.copy(userMessage = null) }
    }
}
