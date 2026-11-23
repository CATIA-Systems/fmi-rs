# fmi-rs for Python

Python bindings for [fmi-rs](../README.md), built with [PyO3](https://pyo3.rs) and
[maturin](https://www.maturin.rs).

```python
import fmi_rs

result = fmi_rs.simulate("BouncingBall.fmu", stop_time=3, output_interval=0.1)
print(result["time"], result["h"])
```

`simulate()` returns a NumPy structured array (like FMPy's `simulate_fmu()`) and accepts
`logger`, `fmi_call_logger` and `step_finished` callbacks to stream log messages, report
progress and stop a running simulation.

## Development

```
pip install maturin numpy pytest
maturin develop
cargo test -p fmi-rs   # downloads the Reference FMUs used by the Python tests
pytest tests
```
