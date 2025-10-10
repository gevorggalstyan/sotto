# Architecture Support

Sotto supports multiple macOS architectures and optimizes performance for each:

## Apple Silicon (M1/M2)

- Uses native Metal GPU acceleration
- Optimized thread count for efficiency cores
- Direct 16kHz audio capture when available

## Intel Macs

- Uses Metal GPU acceleration through dedicated GPU if available
- Falls back to CPU with optimized thread count
- Supports 48kHz → 16kHz audio resampling
- Tested on Intel Core i5/i7/i9 processors
- Supports AMD dedicated GPUs

## System Requirements

- macOS 11.0 or later
- Metal-capable GPU
- Minimum 4GB RAM
- Audio input device

## Performance Considerations

- Apple Silicon: Uses native ARM optimizations
- Intel: Uses x86_64 SIMD instructions where available
- GPU acceleration adapts to available Metal features
- Audio processing automatically adjusts to hardware capabilities

## Development

When contributing, ensure:

1. Test on both architectures if possible
2. Run the test suite: `cargo test`
3. Verify GPU detection works
4. Check audio capture on target platform
