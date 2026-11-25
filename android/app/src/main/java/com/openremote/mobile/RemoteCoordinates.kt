package com.openremote.mobile

import kotlin.math.min

data class NormalizedPoint(val x: Float, val y: Float)
data class ContentRect(val left: Float, val top: Float, val width: Float, val height: Float)

object RemoteCoordinates {
    fun fit(viewWidth: Int, viewHeight: Int, videoWidth: Int, videoHeight: Int): ContentRect? {
        if (viewWidth <= 0 || viewHeight <= 0 || videoWidth <= 0 || videoHeight <= 0) return null
        val scale = min(viewWidth.toFloat() / videoWidth, viewHeight.toFloat() / videoHeight)
        val w = videoWidth * scale; val h = videoHeight * scale
        return ContentRect((viewWidth - w) / 2, (viewHeight - h) / 2, w, h)
    }
    fun map(x: Float, y: Float, rect: ContentRect, clamp: Boolean = false): NormalizedPoint? {
        if (!x.isFinite() || !y.isFinite() || rect.width <= 0 || rect.height <= 0) return null
        val nx = (x - rect.left) / rect.width; val ny = (y - rect.top) / rect.height
        if (!clamp && (nx !in 0f..1f || ny !in 0f..1f)) return null
        return NormalizedPoint(nx.coerceIn(0f, 1f), ny.coerceIn(0f, 1f))
    }
}
