package com.openremote.mobile

import org.junit.Assert.*
import org.junit.Test

class RemoteCoordinatesTest {
    @Test fun portraitPhoneLetterboxingDoesNotMovePointerIntoBlackBars() {
        val rect = RemoteCoordinates.fit(400, 800, 1920, 1080)!!
        assertEquals(225f, rect.height, .001f)
        assertEquals(287.5f, rect.top, .001f)
        assertNull(RemoteCoordinates.map(200f, 50f, rect))
        assertEquals(NormalizedPoint(.5f, .5f), RemoteCoordinates.map(200f, 400f, rect))
    }
    @Test fun dragClampsToVisibleDesktopAndReleaseStaysAtLastPosition() {
        val rect = RemoteCoordinates.fit(400, 800, 1920, 1080)!!
        assertEquals(NormalizedPoint(0f, 1f), RemoteCoordinates.map(-10f, 900f, rect, clamp = true))
    }
    @Test fun invalidDimensionsAndNonFiniteInputAreRejected() {
        assertNull(RemoteCoordinates.fit(0, 800, 1920, 1080))
        assertNull(RemoteCoordinates.map(Float.NaN, 1f, ContentRect(0f,0f,10f,10f)))
    }
}
