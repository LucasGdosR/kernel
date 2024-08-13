// Copyright(c) The Maintainers of Nanvix.
// Licensed under the MIT License.

//==================================================================================================
// Modules
//==================================================================================================

#[cfg(feature = "isapc")]
mod isapc;

#[cfg(feature = "pc")]
mod pc;

//==================================================================================================
// Exports
//==================================================================================================

#[cfg(feature = "isapc")]
pub use isapc::shutdown;

#[cfg(feature = "pc")]
pub use pc::shutdown;

//==================================================================================================
// Standalone Functions
//==================================================================================================

///
/// # Description
///
/// Writes the 8-bit value `b` to the platform's standard output device.
///
/// # Parameters
///
/// - `b`: Value to write.
///
/// # Safety
///
/// This function is unsafe for multiple reasons:
/// - It assumes that the standard output device is present.
/// - It assumes that the standard output device was properly initialized.
/// - It does not prevent concurrent access to the standard output device.
///
pub unsafe fn putb(b: u8) {
    // Base port for UART device.
    const UART_BASE: u16 = 0x3f8;
    // Transmit Holding Register .
    const UART_THR: u16 = UART_BASE;
    // Line Status Register.
    const UART_LSR: u16 = UART_BASE + 5;
    // Transmitter Holding Register empty.
    const UART_LSR_THRE: u8 = 0x20;

    // Wait for the transmit buffer to be empty.
    while (::arch::io::in8(UART_LSR) & UART_LSR_THRE) == 0 {
        // Do nothing
    }

    // Write the byte to the transmit buffer.
    ::arch::io::out8(UART_THR, b);
}
