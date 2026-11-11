package com.swiftget.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.FileDownload
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SuggestionChipDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.swiftget.app.data.model.DownloadItem
import com.swiftget.app.data.model.DownloadState
import com.swiftget.app.ui.theme.SpeedAmber
import com.swiftget.app.ui.theme.SpeedGreen
import com.swiftget.app.ui.theme.SpeedRed

@Composable
fun DownloadCard(
    item: DownloadItem,
    onPause: (Long) -> Unit,
    onResume: (Long) -> Unit,
    onCancel: (Long) -> Unit,
    onRetry: (Long) -> Unit,
    modifier: Modifier = Modifier
) {
    var isExpanded by remember { mutableStateOf(false) }

    Card(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .clickable { isExpanded = !isExpanded },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface
        ),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            // Header: Icon, Filename, State Chip, Expand Icon
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Row(
                    modifier = Modifier.weight(1f),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Default.FileDownload,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(24.dp)
                    )
                    Spacer(modifier = Modifier.width(10.dp))
                    Column {
                        Text(
                            text = item.fileName,
                            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.SemiBold),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                        Text(
                            text = "${item.formattedDownloaded} / ${item.formattedTotal} • ${item.connections} threads",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                }

                StateChip(state = item.state)

                IconButton(
                    onClick = { isExpanded = !isExpanded },
                    modifier = Modifier.size(28.dp)
                ) {
                    Icon(
                        imageVector = if (isExpanded) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown,
                        contentDescription = "Expand details",
                        tint = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }

            // Primary Progress Bar
            if (item.state != DownloadState.COMPLETED) {
                if (item.isIndeterminate) {
                    LinearProgressIndicator(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(6.dp)
                            .clip(RoundedCornerShape(3.dp))
                    )
                } else {
                    LinearProgressIndicator(
                        progress = { item.progressPercent / 100f },
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(6.dp)
                            .clip(RoundedCornerShape(3.dp))
                    )
                }
            }

            // Status Metrics Row: Speed, Percentage, ETA, Controls
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (item.state.isRunning) {
                        Text(
                            text = item.formattedSpeed,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.primary,
                            fontWeight = FontWeight.Bold
                        )
                    }
                    Text(
                        text = "${String.format("%.1f", item.progressPercent)}%",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = "ETA: ${item.formattedEta}",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }

                // Control Action Buttons
                Row(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    when (item.state) {
                        DownloadState.RUNNING, DownloadState.STARTING -> {
                            IconButton(onClick = { onPause(item.id) }, modifier = Modifier.size(32.dp)) {
                                Icon(Icons.Default.Pause, contentDescription = "Pause", tint = SpeedAmber)
                            }
                        }
                        DownloadState.PAUSED -> {
                            IconButton(onClick = { onResume(item.id) }, modifier = Modifier.size(32.dp)) {
                                Icon(Icons.Default.PlayArrow, contentDescription = "Resume", tint = SpeedGreen)
                            }
                        }
                        DownloadState.FAILED -> {
                            IconButton(onClick = { onRetry(item.id) }, modifier = Modifier.size(32.dp)) {
                                Icon(Icons.Default.Refresh, contentDescription = "Retry", tint = SpeedAmber)
                            }
                        }
                        else -> {}
                    }

                    if (item.state != DownloadState.COMPLETED) {
                        IconButton(onClick = { onCancel(item.id) }, modifier = Modifier.size(32.dp)) {
                            Icon(Icons.Default.Close, contentDescription = "Cancel", tint = SpeedRed)
                        }
                    }
                }
            }

            // Per-Segment Progress Bar (always visible if multi-connection and downloading)
            if (item.segments.size > 1 && item.state != DownloadState.COMPLETED) {
                SegmentProgressBar(segments = item.segments)
            }

            // Expanded Section: Real-time Speed Graph
            AnimatedVisibility(
                visible = isExpanded,
                enter = fadeIn() + expandVertically(),
                exit = fadeOut() + shrinkVertically()
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 6.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Text(
                        text = "Throughput History",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    SpeedGraph(
                        speedHistory = item.speedHistory,
                        currentSpeedBps = item.speedBps,
                        graphHeightDp = 90
                    )
                    Text(
                        text = "URL: ${item.url}",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                }
            }
        }
    }
}

@Composable
private fun StateChip(state: DownloadState) {
    val (color, text) = when (state) {
        DownloadState.QUEUED -> MaterialTheme.colorScheme.outline to state.label
        DownloadState.STARTING -> MaterialTheme.colorScheme.primary to state.label
        DownloadState.RUNNING -> SpeedGreen to state.label
        DownloadState.PAUSED -> SpeedAmber to state.label
        DownloadState.COMPLETED -> SpeedGreen to state.label
        DownloadState.FAILED -> SpeedRed to state.label
        DownloadState.CANCELLED -> MaterialTheme.colorScheme.outline to state.label
    }

    AssistChip(
        onClick = {},
        label = {
            Text(
                text = text,
                style = MaterialTheme.typography.labelSmall,
                color = color
            )
        },
        border = SuggestionChipDefaults.suggestionChipBorder(
            enabled = true,
            borderColor = color.copy(alpha = 0.5f)
        )
    )
}
