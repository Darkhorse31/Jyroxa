package com.swiftget.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AudioFile
import androidx.compose.material.icons.filled.Description
import androidx.compose.material.icons.filled.FolderZip
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.InsertDriveFile
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.VideoFile
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.SuggestionChipDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.swiftget.app.data.model.GrabbedLink
import com.swiftget.app.data.model.LinkCategory
import com.swiftget.app.ui.theme.SpeedGreen
import com.swiftget.app.ui.theme.SpeedRed

@Composable
fun LinkItemCard(
    link: GrabbedLink,
    onToggleSelect: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .clickable { onToggleSelect(link.id) },
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surface
        ),
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp)
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(12.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Checkbox(
                checked = link.isSelected,
                onCheckedChange = { onToggleSelect(link.id) }
            )

            Spacer(modifier = Modifier.width(8.dp))

            Icon(
                imageVector = getCategoryIcon(link.category),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(24.dp)
            )

            Spacer(modifier = Modifier.width(12.dp))

            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                Text(
                    text = link.fileName,
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Medium),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )

                Text(
                    text = link.url,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis
                )

                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text(
                        text = link.formattedSize,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                        fontWeight = FontWeight.Bold
                    )

                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        SuggestionChip(
                            onClick = {},
                            label = {
                                Text(
                                    text = link.category.displayName,
                                    style = MaterialTheme.typography.labelSmall
                                )
                            },
                            colors = SuggestionChipDefaults.suggestionChipColors(
                                containerColor = MaterialTheme.colorScheme.surfaceVariant
                            )
                        )

                        val statusColor = if (link.isOnline) SpeedGreen else SpeedRed
                        val statusText = if (link.isOnline) "Online" else "Offline"
                        Text(
                            text = statusText,
                            style = MaterialTheme.typography.labelSmall,
                            color = statusColor,
                            modifier = Modifier.align(Alignment.CenterVertically)
                        )
                    }
                }
            }
        }
    }
}

private fun getCategoryIcon(category: LinkCategory): ImageVector {
    return when (category) {
        LinkCategory.VIDEO -> Icons.Default.VideoFile
        LinkCategory.AUDIO -> Icons.Default.AudioFile
        LinkCategory.ARCHIVE -> Icons.Default.FolderZip
        LinkCategory.DOCUMENT -> Icons.Default.Description
        LinkCategory.IMAGE -> Icons.Default.Image
        LinkCategory.EXECUTABLE, LinkCategory.OTHER, LinkCategory.ALL -> Icons.Default.InsertDriveFile
    }
}
