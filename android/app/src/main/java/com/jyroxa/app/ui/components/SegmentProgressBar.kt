package com.jyroxa.app.ui.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.jyroxa.app.data.model.SegmentProgress
import com.jyroxa.app.ui.theme.SpeedBlue
import com.jyroxa.app.ui.theme.SpeedGreen

/**
 * Visual multi-segment progress bar inspired by IDM / 1DM / ADM.
 * Shows individual chunk progress for each parallel connection.
 */
@Composable
fun SegmentProgressBar(
    segments: List<SegmentProgress>,
    modifier: Modifier = Modifier,
    barHeight: Int = 10
) {
    if (segments.isEmpty()) return

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(barHeight.dp)
                .clip(RoundedCornerShape(4.dp))
                .background(MaterialTheme.colorScheme.surfaceVariant),
            horizontalArrangement = Arrangement.spacedBy(2.dp)
        ) {
            segments.forEach { segment ->
                val animatedProgress by animateFloatAsState(
                    targetValue = (segment.progressPercent / 100f).coerceIn(0f, 1f),
                    animationSpec = tween(durationMillis = 300, easing = FastOutSlowInEasing),
                    label = "segment_progress_${segment.segmentIndex}"
                )

                Box(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .background(MaterialTheme.colorScheme.surfaceVariant)
                ) {
                    val fillColor = if (segment.isCompleted) {
                        SpeedGreen
                    } else {
                        SpeedBlue
                    }

                    Box(
                        modifier = Modifier
                            .fillMaxWidth(animatedProgress)
                            .fillMaxHeight()
                            .clip(RoundedCornerShape(2.dp))
                            .background(fillColor)
                    )
                }
            }
        }

        // Segment status sub-caption
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            val completedCount = segments.count { it.isCompleted }
            Text(
                text = "${segments.size} connections (${completedCount}/${segments.size} chunks done)",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )

            val activeSpeedSegments = segments.filter { !it.isCompleted }
            if (activeSpeedSegments.isNotEmpty()) {
                Text(
                    text = "Parallel multi-part",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary
                )
            }
        }
    }
}
