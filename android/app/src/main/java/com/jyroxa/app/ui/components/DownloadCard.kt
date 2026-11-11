package com.jyroxa.app.ui.components

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.widget.Toast
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material.icons.automirrored.filled.InsertDriveFile
import androidx.compose.material.icons.filled.Android
import androidx.compose.material.icons.filled.AudioFile
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.FolderZip
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.VideoFile
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.jyroxa.app.data.model.DownloadItem
import com.jyroxa.app.data.model.DownloadState
import com.jyroxa.app.data.model.LinkCategory
import com.jyroxa.app.ui.theme.SpeedAmber
import com.jyroxa.app.ui.theme.SpeedBlue
import com.jyroxa.app.ui.theme.SpeedGreen
import com.jyroxa.app.ui.theme.SpeedRed
import java.util.Locale

@Composable
fun DownloadCard(
    item: DownloadItem,
    onPause: (Long) -> Unit,
    onResume: (Long) -> Unit,
    onCancel: (Long) -> Unit,
    onRetry: (Long) -> Unit,
    modifier: Modifier = Modifier
) {
    val context = LocalContext.current
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
            // Header: Category Icon, Filename, Size, Status Pill, Expand Icon
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Row(
                    modifier = Modifier.weight(1f),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    FileTypeThumbnail(category = item.fileCategory)
                    Spacer(modifier = Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
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

                Spacer(modifier = Modifier.width(8.dp))
                StatusPill(state = item.state)

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

            // Status Metrics Row: Speed + Avg, Percentage, EWMA ETA, Control Buttons
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Row(
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.weight(1f, fill = false)
                ) {
                    if (item.state.isRunning) {
                        Text(
                            text = item.formattedSpeedAndAvg,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.primary,
                            fontWeight = FontWeight.Bold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                    Text(
                        text = "${String.format(Locale.US, "%.1f", item.progressPercent)}%",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface,
                        fontWeight = FontWeight.SemiBold
                    )
                    Text(
                        text = item.formattedEta,
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

            // Segment Overview Bar (when multi-connection)
            if (item.segments.size > 1 && item.state != DownloadState.COMPLETED) {
                SegmentProgressBar(segments = item.segments)
            }

            // Expanded Section: Real-time Speed Graph + Per-Segment Details
            AnimatedVisibility(
                visible = isExpanded,
                enter = fadeIn() + expandVertically(),
                exit = fadeOut() + shrinkVertically()
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp)
                ) {
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))

                    Text(
                        text = "Transfer Throughput",
                        style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Bold),
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )

                    SpeedGraph(
                        speedHistory = item.speedHistory,
                        currentSpeedBps = item.speedBps,
                        graphHeightDp = 90
                    )

                    // URL row with copy button
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween
                    ) {
                        Text(
                            text = item.url,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.8f),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f)
                        )
                        IconButton(
                            onClick = {
                                val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                val clip = ClipData.newPlainText("Download URL", item.url)
                                clipboard.setPrimaryClip(clip)
                                Toast.makeText(context, "URL copied", Toast.LENGTH_SHORT).show()
                            },
                            modifier = Modifier.size(24.dp)
                        ) {
                            Icon(
                                Icons.Default.ContentCopy,
                                contentDescription = "Copy URL",
                                tint = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.size(16.dp)
                            )
                        }
                    }

                    // Detailed Per-Segment Chunk Breakdown
                    if (item.segments.isNotEmpty()) {
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
                        Text(
                            text = "Parallel Segment Breakdown (${item.segments.size} connections)",
                            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Bold),
                            color = MaterialTheme.colorScheme.onSurface
                        )

                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            item.segments.forEach { seg ->
                                val segLength = (seg.endByte - seg.startByte).coerceAtLeast(0L)
                                val segDownloaded = seg.downloadedBytes.coerceAtMost(segLength)
                                val segPct = if (segLength > 0) ((segDownloaded.toDouble() / segLength) * 100).toFloat() else 0f

                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clip(RoundedCornerShape(8.dp))
                                        .padding(vertical = 2.dp),
                                    verticalArrangement = Arrangement.spacedBy(2.dp)
                                ) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                        verticalAlignment = Alignment.CenterVertically
                                    ) {
                                        Text(
                                            text = "Segment #${seg.segmentIndex + 1} (${DownloadItem.formatBytes(seg.startByte)} - ${DownloadItem.formatBytes(seg.endByte)})",
                                            style = MaterialTheme.typography.labelSmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant
                                        )
                                        Text(
                                            text = if (seg.isCompleted) "Done" else "${String.format(Locale.US, "%.1f", segPct)}%",
                                            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold),
                                            color = if (seg.isCompleted) SpeedGreen else MaterialTheme.colorScheme.primary
                                        )
                                    }
                                    LinearProgressIndicator(
                                        progress = { (segPct / 100f).coerceIn(0f, 1f) },
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .height(4.dp)
                                            .clip(RoundedCornerShape(2.dp)),
                                        color = if (seg.isCompleted) SpeedGreen else SpeedBlue,
                                        trackColor = MaterialTheme.colorScheme.surfaceVariant
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun FileTypeThumbnail(
    category: LinkCategory,
    modifier: Modifier = Modifier
) {
    val (icon, bgTint, iconTint) = when (category) {
        LinkCategory.VIDEO -> Triple(Icons.Default.VideoFile, Color(0xFFEADDFF), Color(0xFF6750A4))
        LinkCategory.AUDIO -> Triple(Icons.Default.AudioFile, Color(0xFFFFD8E4), Color(0xFF7D5260))
        LinkCategory.IMAGE -> Triple(Icons.Default.Image, Color(0xFFC4EED0), Color(0xFF0F5223))
        LinkCategory.ARCHIVE -> Triple(Icons.Default.FolderZip, Color(0xFFFFE0B2), Color(0xFFE65100))
        LinkCategory.DOCUMENT -> Triple(Icons.Default.Description, Color(0xFFD3E3FD), Color(0xFF041E49))
        LinkCategory.EXECUTABLE -> Triple(Icons.Default.Android, Color(0xFFC7EBF0), Color(0xFF004D61))
        else -> Triple(
            Icons.AutoMirrored.Filled.InsertDriveFile,
            MaterialTheme.colorScheme.surfaceVariant,
            MaterialTheme.colorScheme.onSurfaceVariant
        )
    }

    Surface(
        shape = RoundedCornerShape(10.dp),
        color = bgTint,
        modifier = modifier.size(40.dp)
    ) {
        Box(contentAlignment = Alignment.Center) {
            Icon(
                imageVector = icon,
                contentDescription = category.displayName,
                tint = iconTint,
                modifier = Modifier.size(24.dp)
            )
        }
    }
}

@Composable
private fun StatusPill(state: DownloadState) {
    val (bgColor, textColor) = when (state) {
        DownloadState.RUNNING -> SpeedGreen.copy(alpha = 0.15f) to SpeedGreen
        DownloadState.COMPLETED -> SpeedGreen.copy(alpha = 0.15f) to SpeedGreen
        DownloadState.STARTING -> MaterialTheme.colorScheme.primaryContainer to MaterialTheme.colorScheme.onPrimaryContainer
        DownloadState.PAUSED -> SpeedAmber.copy(alpha = 0.15f) to SpeedAmber
        DownloadState.FAILED -> SpeedRed.copy(alpha = 0.15f) to SpeedRed
        DownloadState.QUEUED -> MaterialTheme.colorScheme.surfaceVariant to MaterialTheme.colorScheme.onSurfaceVariant
        DownloadState.CANCELLED -> MaterialTheme.colorScheme.surfaceVariant to MaterialTheme.colorScheme.outline
    }

    Surface(
        shape = RoundedCornerShape(12.dp),
        color = bgColor,
        modifier = Modifier.padding(horizontal = 2.dp)
    ) {
        Text(
            text = state.label,
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold),
            color = textColor,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
        )
    }
}
