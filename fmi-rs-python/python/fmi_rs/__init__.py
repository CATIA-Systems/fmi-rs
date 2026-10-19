"""Simulate Functional Mock-up Units (FMUs) with fmi-rs."""

from __future__ import annotations

import os
from collections.abc import Callable, Iterable, Mapping
from typing import Any

import numpy as np

from ._native import FmiRsError
from ._native import simulate as _simulate

__all__ = ["FmiRsError", "simulate"]

Logger = Callable[[int, str, str], Any]
"""Receives the FMU's log messages: ``logger(status, category, message)``."""

FmiCallLogger = Callable[[str], Any]
"""Receives the logged FMI calls: ``fmi_call_logger(message)``."""

StepFinished = Callable[[float, tuple], bool]
"""Called after each sample with ``(time, (time, value, ...))``. Return ``False`` to stop."""


def _literal(value: Any) -> str:
    """Converts a start value to the literal expected by fmi-rs."""
    if isinstance(value, (bool, np.bool_)):
        return "true" if value else "false"
    if isinstance(value, (bytes, bytearray)):
        return value.hex()
    if isinstance(value, (list, tuple, np.ndarray)):
        return " ".join(_literal(v) for v in value)
    return str(value)


def _to_array(columns: list[tuple[str, str, bool]], rows: list[tuple]) -> np.ndarray:
    dtype: list[tuple] = [("time", "f8")]

    for i, (name, kind, is_array) in enumerate(columns, start=1):
        if is_array:
            size = len(rows[0][i]) if rows else 0
            dtype.append((name, kind, (size,)))
        else:
            dtype.append((name, kind))

    return np.array(rows, dtype=np.dtype(dtype))


def simulate(
    filename: str | os.PathLike,
    *,
    fmi_type: str | None = None,
    start_time: float | None = None,
    stop_time: float | None = None,
    output_interval: float | None = None,
    solver: str = "CVode",
    step_size: float | None = None,
    relative_tolerance: float | None = None,
    start_values: Mapping[str, Any] | None = None,
    input_file: str | os.PathLike | None = None,
    output: Iterable[str] | None = None,
    debug_logging: bool = False,
    set_stop_time: bool = True,
    enable_dae: bool = False,
    logger: Logger | None = None,
    fmi_call_logger: FmiCallLogger | None = None,
    step_finished: StepFinished | None = None,
) -> np.ndarray:
    """Simulates an FMU and returns the result as a NumPy structured array.

    Parameters
    ----------
    filename:
        Path to the FMU file or to the directory it was extracted to.
    fmi_type:
        ``"CoSimulation"`` or ``"ModelExchange"`` (default: Co-Simulation if available).
    start_time, stop_time, output_interval, relative_tolerance:
        Default to the FMU's ``DefaultExperiment``. The output interval must divide the
        simulation duration.
    solver:
        Model Exchange solver: ``"Euler"``, ``"CVode"`` or ``"IDA"``.
    step_size:
        Fixed step size of the Euler solver (default: ``output_interval``).
    start_values:
        Start values by variable name. Arrays are given as sequences.
    input_file:
        CSV file with the time in the first column and one column per input variable.
    output:
        Names of the variables to record (default: all outputs).
    step_finished:
        Called after each recorded sample. Returning ``False`` stops the simulation and the
        samples recorded so far are returned. An exception raised by the callback stops the
        simulation and is re-raised.

    Raises
    ------
    FmiRsError
        If the simulation fails.
    """
    columns, rows, _cancelled = _simulate(
        os.fspath(filename),
        fmi_type=fmi_type,
        start_time=start_time,
        stop_time=stop_time,
        output_interval=output_interval,
        solver=solver,
        step_size=step_size,
        relative_tolerance=relative_tolerance,
        start_values=[(name, _literal(value)) for name, value in (start_values or {}).items()],
        input_file=os.fspath(input_file) if input_file is not None else None,
        output=list(output) if output is not None else None,
        debug_logging=debug_logging,
        set_stop_time=set_stop_time,
        enable_dae=enable_dae,
        logger=logger,
        fmi_call_logger=fmi_call_logger,
        step_finished=step_finished,
    )

    return _to_array(columns, rows)
