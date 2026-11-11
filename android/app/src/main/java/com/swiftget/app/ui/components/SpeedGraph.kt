package com.swiftget.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import com.swiftget.app.data.model.DownloadItem
import com.swiftget.app.data.model.SpeedSample
import com.swiftget.app.ui.theme.SpeedCyan

/**
 * Real-time Canvas speed graph rendering network throughput history.
 */
@Composable
fun SpeedGraph(
    speedHistory: List<SpeedSample>,
    currentSpeedBps: Double,
    modifier: Modifier = Modifier,
    graphHeightDp: Int = 80
) {
    val primaryColor = MaterialTheme.colorScheme.primary
    val gridColor = MaterialTheme.colorScheme.outline.copy(alpha = 0.2f)
    val cyanColor = SpeedCyan

    val maxSpeed = remember(speedHistory, currentSpeedBps) {
        val historicalMax = speedHistory.maxOfOrNull { it.speedBps } ?: 0.0
        maxOf(historicalMax, currentSpeedBps, 1024.0 * 1024.0) // minimum 1 MB/s ceiling
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(graphHeightDp.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
    ) {
        Canvas(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 4.dp, vertical = 6.dp)
        ) {
            val width = size.width
            val height = size.height

            if (width <= 0 || height <= 0) return@Canvas

            // 1. Draw horizontal grid guide lines (25%, 50%, 75%)
            for (i in 1..3) {
                val y = height * (i / 4f)
                drawLine(
                    color = gridColor,
                    start = Offset(0f, y),
                    end = Offset(width, y),
                    strokeWidth = 1f
                )
            }

            if (speedHistory.size < 2) {
                // Baseline line if insufficient data
                drawLine(
                    color = cyanColor.copy(alpha = 0.5f),
                    start = Offset(0f, height),
                    end = Offset(width, height),
                    strokeWidth = 2f
                )
                return@Canvas
            }

            // 2. Generate points
            val stepX = width / (speedHistory.size - 1).coerceAtLeast(1)
            val strokePath = Path()
            val fillPath = Path()

            speedHistory.forEachIndexed { index, sample ->
                val x = index * stepX
                val normalizedY = (sample.speedBps / maxSpeed).coerceIn(0.0, 1.0).toFloat()
                val y = height - (normalizedY * (height - 4f))

                if (index == 0) {
                    strokePath.moveTo(x, y)
                    fillPath.moveTo(x, height)
                    fillPath.lineTo(x, y)
                } else {
                    strokePath.lineTo(x, y)
                    fillPath.lineTo(x, y)
                }
            }

            // Close fill area back to bottom edge
            fillPath.lineTo(width, height)
            fillPath.close()

            // 3. Draw gradient area fill
            drawPath(
                path = fillPath,
                brush = Brush.verticalGradient(
                    colors = listOf(
                        cyanColor.copy(alpha = 0.35f),
                        primaryColor.copy(alpha = 0.05f)
                    ),
                    startY = 0f,
                    endY = height
                )
            )

            // 4. Draw smooth stroke line
            drawPath(
                path = strokePath,
                color = cyanColor,
                style = Stroke(
                    width = 2.5.dp.toPx(),
                    cap = StrokeCap.Round
                )
            )

            // 5. Draw glowing current speed point at the latest sample
            val lastNormalized = (currentSpeedBps / maxSpeed).coerceIn(0.0, 1.0).toFloat()
            val lastY = height - (lastNormalized * (height - 4f))
            drawCircle(
                color = cyanColor,
                radius = 4.dp.toPx(),
                center = Offset(width, lastY)
            )
            drawCircle(
                color = Color.White,
                radius = 2.dp.toPx(),
                center = Offset(width, lastY)
            )
        }

        // Top labels: Max speed and current live speed
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 8.dp, vertical = 4.dp)
        ) {
            Text(
                text = "Peak: ${DownloadItem.formatBytesSpeed(maxSpeed)}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                modifier = Modifier.align(Alignment.TopStart)
            )

            Text(
                text = DownloadItem.formatBytesSpeed(currentSpeedBps),
                style = MaterialTheme.typography.labelSmall,
                color = SpeedCyan,
                modifier = Modifier.align(Alignment.TopEnd)
            )
        }
    }
}
