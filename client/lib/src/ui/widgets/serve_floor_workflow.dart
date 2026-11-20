import 'dart:async';

import 'package:flutter/material.dart';

import '../../api/control_model.dart';
import '../../api/read_model.dart';
import '../../state/client_controller.dart';
import '../../state/client_state.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

/// Read-side control presentation bound to a single resolved operational car card.
/// Never attached to read-only Hosts or configured-but-unavailable cars.
@immutable
final class CarControlPresentation {
  const CarControlPresentation({
    required this.surface,
    this.carCapability,
    this.trackedForCar,
    required this.hostLive,
  });

  final ControlSurfaceCapabilities surface;
  final CarServeFloorCapability? carCapability;
  final TrackedServeFloorOperation? trackedForCar;
  final bool hostLive;
}

(String, Color, Color, Color, bool) describeProjectedOperationStatus(
  TrackedServeFloorOperation operation,
  InstrumentPalette palette,
) {
  if (operation.submitInFlight) {
    return (
      'Submitting',
      palette.info,
      palette.infoContainer,
      palette.info,
      true,
    );
  }
  if (operation.conflictMessage != null) {
    return (
      'Conflict',
      palette.danger,
      palette.dangerContainer,
      palette.danger,
      false,
    );
  }
  if (operation.validationErrorMessage != null) {
    return (
      'Rejected',
      palette.danger,
      palette.dangerContainer,
      palette.danger,
      false,
    );
  }
  switch (operation.effectiveStatus) {
    case ProjectedOperationStatus.accepted:
      return (
        'Accepted',
        palette.info,
        palette.infoContainer,
        palette.info,
        true,
      );
    case ProjectedOperationStatus.inProgress:
      return (
        'In progress',
        palette.info,
        palette.infoContainer,
        palette.info,
        false,
      );
    case ProjectedOperationStatus.completed:
      return ('Completed', palette.ok, palette.okContainer, palette.ok, false);
    case ProjectedOperationStatus.diverged:
      return (
        'Diverged',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        false,
      );
    case ProjectedOperationStatus.rejected:
      return (
        'Rejected',
        palette.danger,
        palette.dangerContainer,
        palette.danger,
        false,
      );
    case ProjectedOperationStatus.notFound:
      return (
        'Unknown (not found)',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        true,
      );
    case ProjectedOperationStatus.knowledgeUnavailable:
      return (
        'Unknown',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        true,
      );
  }
}

String describeOperationTruthCaption(TrackedServeFloorOperation operation) {
  if (operation.submitInFlight) {
    return 'Sending immutable ServeFloor submission to Simulator control path...';
  }
  if (operation.conflictMessage != null) {
    return 'Correlation key conflict: ${operation.conflictMessage} No second operation was created.';
  }
  if (operation.validationErrorMessage != null) {
    return 'Request rejected before admission: ${operation.validationErrorMessage}';
  }

  final ProjectedSubmission? proj = operation.projection;
  switch (operation.effectiveStatus) {
    case ProjectedOperationStatus.accepted:
      return 'Accepted by Engine (receipt: ${proj?.receipt ?? "awaiting"}). Acceptance does not confirm physical arrival or completion.';
    case ProjectedOperationStatus.inProgress:
      return 'In progress: Engine observed physical progress via canonical Operational observations (reconciliation: pending).';
    case ProjectedOperationStatus.completed:
      return 'Completed: Engine canonical reconciliation established physical satisfaction (reconciliation: satisfied).';
    case ProjectedOperationStatus.diverged:
      final String observed = proj?.divergedFloor != null
          ? 'Floor ${proj!.divergedFloor}'
          : 'another floor';
      return 'Diverged: Engine reconciliation observed car stopped at $observed instead of Floor ${operation.floor}.';
    case ProjectedOperationStatus.rejected:
      final ServeFloorBlocker? blocker = proj?.blocker;
      if (blocker != null) {
        return 'Rejected (${blocker.code}): ${blocker.detail}. This submission key is terminal rejected and will not be retried automatically.';
      }
      return 'Rejected by Engine/controller. This submission key is terminal rejected and will not be retried automatically.';
    case ProjectedOperationStatus.notFound:
      return 'No known submission for this correlation key in the current control process lifetime. No automatic resend.';
    case ProjectedOperationStatus.knowledgeUnavailable:
      final String? reason = operation.effectiveKnowledgeReason;
      return switch (reason) {
        'response_lost' => 'Submission response was lost after dispatch. The server may have accepted the request. No automatic resend will occur; query status after reconnecting.',
        'control_process_restarted' => 'Prior submission knowledge unavailable: the development control process restarted and ephemeral state is not persisted. No automatic resend occurred.',
        'delivery_or_receipt_indeterminate' => 'Delivery or controller receipt is indeterminate in Engine; awaiting canonical observation evidence. No automatic resend.',
        'engine_history_evicted' || 'submission_evicted_or_unknown' => 'Prior submission knowledge is no longer retained in bounded server memory. No automatic resend.',
        _ => 'Current submission knowledge is unavailable. Treated as Unknown without automatic resend.',
      };
  }
}

String describeAuthorizationKind(ControlAuthorizationKind kind) {
  return switch (kind) {
    ControlAuthorizationKind.devProcessLocalGrant =>
      'Dev process-local grant (non-production)',
    ControlAuthorizationKind.fenced => 'Authority fenced / revoked',
    ControlAuthorizationKind.unbound => 'No controller authority bound',
  };
}

String describeSupport(OperationSupport support) {
  return switch (support) {
    OperationSupport.supported => 'Supported',
    OperationSupport.unsupported => 'Unsupported',
    OperationSupport.unknown => 'Support unknown',
  };
}

String describePathAvailability(
  ControlPathAvailability availability,
  String? reason,
) {
  if (availability == ControlPathAvailability.available) {
    return 'Available';
  }
  return reason != null && reason.isNotEmpty
      ? 'Unavailable ($reason)'
      : 'Unavailable';
}

/// Compact workspace banner surfacing the currently tracked ServeFloor submission
/// across modal closes, disconnects, reconnects, and Host restarts.
class TrackedOperationBanner extends StatelessWidget {
  const TrackedOperationBanner({
    super.key,
    required this.operation,
    required this.isHostLive,
    required this.onInspectDetails,
    required this.onQueryStatus,
    required this.onDismiss,
  });

  final TrackedServeFloorOperation operation;
  final bool isHostLive;
  final VoidCallback onInspectDetails;
  final VoidCallback onQueryStatus;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (
      String statusLabel,
      Color fg,
      Color bg,
      Color dotColor,
      bool hollowDot,
    ) = describeProjectedOperationStatus(
      operation,
      palette,
    );
    final ProjectedSubmission? proj = operation.projection;
    final String intentLabel = proj?.intentId != null
        ? 'IntentId #${proj!.intentId}'
        : 'IntentId: none';

    return DecoratedBox(
      key: const Key('tracked-operation-banner'),
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: palette.border),
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(5),
        child: DecoratedBox(
          position: DecorationPosition.foreground,
          decoration: BoxDecoration(
            border: Border(left: BorderSide(color: fg, width: 2.5)),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: <Widget>[
                    SemanticBadge(
                      label: statusLabel,
                      foreground: fg,
                      background: bg,
                      dotColor: dotColor,
                      hollowDot: hollowDot,
                      outlined: true,
                    ),
                    Text(
                      'ServeFloor · ${operation.car} → ${operation.floor}F',
                      style: instrumentMonoStyle(
                        context,
                        fontSize: 12.5,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    Text(
                      intentLabel,
                      style: instrumentMonoStyle(
                        context,
                        fontSize: 11,
                        fontWeight: FontWeight.w600,
                        color: palette.subtleText,
                      ),
                    ),
                    if (proj?.duplicateSubmission == true)
                      SemanticBadge(
                        label: 'Idempotent replay',
                        foreground: palette.subtleText,
                        background: palette.neutralContainer,
                        outlined: true,
                      ),
                  ],
                ),
                const SizedBox(height: 5),
                Text(
                  describeOperationTruthCaption(operation),
                  style: TextStyle(
                    fontSize: 11.5,
                    height: 1.35,
                    color: palette.subtleText,
                  ),
                ),
                const SizedBox(height: 5),
                Wrap(
                  spacing: 10,
                  runSpacing: 4,
                  children: <Widget>[
                    Text(
                      'Key: ${operation.submissionKey}',
                      style: instrumentMonoStyle(
                        context,
                        fontSize: 10.5,
                        color: palette.mutedText,
                      ),
                    ),
                    if (proj?.receipt != null)
                      Text(
                        'Receipt: ${proj!.receipt}',
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 10.5,
                          color: palette.mutedText,
                        ),
                      ),
                    if (proj?.reconciliation != null)
                      Text(
                        'Reconciliation: ${proj!.reconciliation}',
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 10.5,
                          color: palette.mutedText,
                        ),
                      ),
                  ],
                ),
                if (operation.lastQueryError != null) ...<Widget>[
                  const SizedBox(height: 4),
                  Text(
                    operation.lastQueryError!,
                    style: TextStyle(fontSize: 11, color: palette.caution),
                  ),
                ],
                const SizedBox(height: 8),
                Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  children: <Widget>[
                    OutlinedButton(
                      key: const Key('tracked-op-details-button'),
                      onPressed: onInspectDetails,
                      style: OutlinedButton.styleFrom(
                        minimumSize: const Size(0, 30),
                        padding: const EdgeInsets.symmetric(
                          horizontal: 10,
                          vertical: 4,
                        ),
                        textStyle: const TextStyle(fontSize: 11.5),
                      ),
                      child: const Text('Details'),
                    ),
                    OutlinedButton(
                      key: const Key('tracked-op-query-button'),
                      onPressed:
                          isHostLive &&
                              !operation.submitInFlight &&
                              !operation.queryInFlight
                          ? onQueryStatus
                          : null,
                      style: OutlinedButton.styleFrom(
                        minimumSize: const Size(0, 30),
                        padding: const EdgeInsets.symmetric(
                          horizontal: 10,
                          vertical: 4,
                        ),
                        textStyle: const TextStyle(fontSize: 11.5),
                      ),
                      child: Text(
                        operation.queryInFlight
                            ? 'Querying...'
                            : 'Query status',
                      ),
                    ),
                    TextButton(
                      key: const Key('tracked-op-dismiss-button'),
                      onPressed: operation.submitInFlight ? null : onDismiss,
                      style: TextButton.styleFrom(
                        minimumSize: const Size(0, 30),
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 4,
                        ),
                        textStyle: const TextStyle(fontSize: 11.5),
                      ),
                      child: const Text('Dismiss'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

Future<void> showServeFloorWorkflowModal({
  required BuildContext context,
  required ClientController controller,
  required String carId,
  bool startInStatusView = false,
}) async {
  final bool isCompact = MediaQuery.sizeOf(context).width < 600;
  if (isCompact) {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      backgroundColor: context.palette.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(10)),
      ),
      builder: (BuildContext sheetContext) {
        return Padding(
          padding: EdgeInsets.only(
            bottom: MediaQuery.viewInsetsOf(sheetContext).bottom,
          ),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(16),
            child: _ServeFloorWorkflowBody(
              controller: controller,
              carId: carId,
              startInStatusView: startInStatusView,
              onClose: () => Navigator.of(sheetContext).pop(),
            ),
          ),
        );
      },
    );
    return;
  }

  await showDialog<void>(
    context: context,
    builder: (BuildContext dialogContext) {
      return Dialog(
        backgroundColor: dialogContext.palette.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(color: dialogContext.palette.border),
        ),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(18),
            child: _ServeFloorWorkflowBody(
              controller: controller,
              carId: carId,
              startInStatusView: startInStatusView,
              onClose: () => Navigator.of(dialogContext).pop(),
            ),
          ),
        ),
      );
    },
  );
}

class _ServeFloorWorkflowBody extends StatefulWidget {
  const _ServeFloorWorkflowBody({
    required this.controller,
    required this.carId,
    required this.startInStatusView,
    required this.onClose,
  });

  final ClientController controller;
  final String carId;
  final bool startInStatusView;
  final VoidCallback onClose;

  @override
  State<_ServeFloorWorkflowBody> createState() =>
      _ServeFloorWorkflowBodyState();
}

class _ServeFloorWorkflowBodyState extends State<_ServeFloorWorkflowBody> {
  bool _inStatusView = false;
  int? _selectedFloor;
  String? _draftSubmissionKey;
  bool _previewLoading = false;
  ServeFloorPreview? _preview;
  String? _previewError;
  int _previewGeneration = 0;

  @override
  void initState() {
    super.initState();
    _inStatusView = widget.startInStatusView;
  }

  Future<void> _selectFloorAndPreview(int floor) async {
    final int gen = ++_previewGeneration;
    final String freshKey = widget.controller.mintSubmissionKey();
    setState(() {
      _inStatusView = false;
      _selectedFloor = floor;
      _draftSubmissionKey = freshKey;
      _previewLoading = true;
      _preview = null;
      _previewError = null;
    });

    try {
      final ServeFloorPreview result = await widget.controller
          .previewServeFloor(car: widget.carId, floor: floor);
      if (!mounted || gen != _previewGeneration) {
        return;
      }
      setState(() {
        _previewLoading = false;
        _preview = result;
        _previewError = null;
      });
    } catch (error) {
      if (!mounted || gen != _previewGeneration) {
        return;
      }
      setState(() {
        _previewLoading = false;
        _preview = null;
        _previewError = '$error';
      });
    }
  }

  void _startNewAttempt() {
    _previewGeneration++;
    setState(() {
      _inStatusView = false;
      _selectedFloor = null;
      _draftSubmissionKey = null;
      _previewLoading = false;
      _preview = null;
      _previewError = null;
    });
  }

  Future<void> _submitTransaction() async {
    final int? floor = _selectedFloor;
    final String? key = _draftSubmissionKey;
    final ServeFloorPreview? preview = _preview;
    if (floor == null || key == null || preview == null || !preview.eligible) {
      return;
    }
    setState(() {
      _inStatusView = true;
    });
    await widget.controller.submitServeFloor(
      submissionKey: key,
      car: widget.carId,
      floor: floor,
      expectedControlInstanceId: preview.controlInstanceId,
    );
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    return ListenableBuilder(
      listenable: widget.controller,
      builder: (BuildContext context, _) {
        final ClientState state = widget.controller.state;
        final ControlSurfaceCapabilities? surface = state.controlCapabilities;
        final CarServeFloorCapability? cap = switch (surface) {
          ControlSurfaceAvailable() => surface.capabilityForCar(widget.carId),
          _ => null,
        };
        final OperationalCar? liveCar = state.snapshot?.operationalCars
            .cast<OperationalCar?>()
            .firstWhere(
              (OperationalCar? c) => c?.id == widget.carId,
              orElse: () => null,
            );
        final TrackedServeFloorOperation? tracked = state.trackedOperation;

        return Column(
          key: const Key('serve-floor-workflow-modal'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            // Header
            Row(
              children: <Widget>[
                Expanded(
                  child: Wrap(
                    spacing: 8,
                    runSpacing: 4,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: <Widget>[
                      Text(
                        'ServeFloor · ${widget.carId}',
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 14.5,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      SemanticBadge(
                        label: 'dev-sim',
                        foreground: palette.info,
                        background: palette.infoContainer,
                        outlined: true,
                        tooltip: 'Development-only Simulator control surface; read API remains read-only.',
                      ),
                    ],
                  ),
                ),
                IconButton(
                  key: const Key('serve-floor-modal-close'),
                  visualDensity: VisualDensity.compact,
                  onPressed: widget.onClose,
                  icon: const Icon(Icons.close, size: 17),
                ),
              ],
            ),
            const SizedBox(height: 8),

            if (_inStatusView && tracked != null)
              _buildStatusView(context, state, tracked, liveCar)
            else
              _buildPrepareAndReviewView(context, state, surface, cap, liveCar),
          ],
        );
      },
    );
  }

  Widget _buildPrepareAndReviewView(
    BuildContext context,
    ClientState state,
    ControlSurfaceCapabilities? surface,
    CarServeFloorCapability? cap,
    OperationalCar? liveCar,
  ) {
    final InstrumentPalette palette = context.palette;

    if (liveCar == null) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            'Car ${widget.carId} currently has no valid Operational observation state.',
            style: TextStyle(
              fontSize: 12.5,
              fontWeight: FontWeight.w600,
              color: palette.caution,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            'ServeFloor preview and submission require valid Operational truth. Auxiliary bindings cannot restore command eligibility.',
            style: TextStyle(
              fontSize: 11.5,
              height: 1.35,
              color: palette.subtleText,
            ),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerRight,
            child: OutlinedButton(
              onPressed: widget.onClose,
              child: const Text('Close'),
            ),
          ),
        ],
      );
    }

    if (surface is! ControlSurfaceAvailable || cap == null) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            surface is ControlSurfaceSupportUnknown
                ? 'Control support unknown: ${surface.reason}'
                : 'ServeFloor control capability is not available for ${widget.carId}.',
            style: TextStyle(fontSize: 12, color: palette.subtleText),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: Alignment.centerRight,
            child: OutlinedButton(
              onPressed: widget.onClose,
              child: const Text('Close'),
            ),
          ),
        ],
      );
    }

    final bool canPreviewFloors =
        cap.support == OperationSupport.supported &&
        cap.controlPathAvailability == ControlPathAvailability.available &&
        cap.executableFloors.isNotEmpty;

    final bool canSubmit =
        state.isSnapshotLive &&
        !_previewLoading &&
        _selectedFloor != null &&
        _draftSubmissionKey != null &&
        _preview != null &&
        _preview!.eligible;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        // Explicit separated capability dimensions (supported != available != authorized != eligible)
        DecoratedBox(
          decoration: BoxDecoration(
            color: palette.elevatedSurface,
            borderRadius: BorderRadius.circular(5),
            border: Border.all(color: palette.border),
          ),
          child: Padding(
            padding: const EdgeInsets.all(10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                _KvLine(
                  label: 'Support',
                  value: describeSupport(cap.support),
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Control path',
                  value: describePathAvailability(
                    cap.controlPathAvailability,
                    cap.controlPathReason,
                  ),
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Authority',
                  value: describeAuthorizationKind(cap.authorization),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),

        Text(
          'Select target floor (from ExecutionCapabilities)',
          style: TextStyle(
            fontSize: 11.5,
            fontWeight: FontWeight.w600,
            color: palette.subtleText,
          ),
        ),
        const SizedBox(height: 6),

        if (cap.executableFloors.isEmpty)
          Text(
            'No executable floors reported by ExecutionCapabilities for ${widget.carId}.',
            style: TextStyle(fontSize: 11.5, color: palette.caution),
          )
        else
          Wrap(
            key: const Key('serve-floor-selector'),
            spacing: 6,
            runSpacing: 6,
            children: <Widget>[
              for (final int floor in cap.executableFloors)
                ChoiceChip(
                  key: Key('serve-floor-choice-$floor'),
                  label: Text(
                    '${floor}F',
                    style: instrumentMonoStyle(
                      context,
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  selected: _selectedFloor == floor,
                  onSelected: canPreviewFloors && state.isSnapshotLive
                      ? (_) => unawaited(_selectFloorAndPreview(floor))
                      : null,
                ),
            ],
          ),

        const SizedBox(height: 12),

        // Step 2: Transaction-bound Advisory Preview & Review
        if (_selectedFloor != null) ...<Widget>[
          DecoratedBox(
            key: const Key('serve-floor-review-panel'),
            decoration: BoxDecoration(
              color: palette.elevatedSurface,
              borderRadius: BorderRadius.circular(5),
              border: Border.all(color: palette.border),
            ),
            child: Padding(
              padding: const EdgeInsets.all(11),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  const Text(
                    'Transaction Review',
                    style: TextStyle(
                      fontSize: 12.5,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    'Server preview is advisory only. Engine re-checks admission when submitted.',
                    style: TextStyle(fontSize: 11, color: palette.mutedText),
                  ),
                  const SizedBox(height: 8),
                  _KvLine(
                    label: 'Operation',
                    value: 'ServeFloor',
                    monospace: true,
                  ),
                  const SizedBox(height: 3),
                  _KvLine(
                    label: 'Logical car',
                    value: widget.carId,
                    monospace: true,
                  ),
                  const SizedBox(height: 3),
                  _KvLine(
                    label: 'Target floor',
                    value: '${_selectedFloor}F',
                    monospace: true,
                  ),
                  if (_draftSubmissionKey != null) ...<Widget>[
                    const SizedBox(height: 3),
                    _KvLine(
                      label: 'Correlation key',
                      value: _draftSubmissionKey!,
                      monospace: true,
                    ),
                  ],
                  const SizedBox(height: 8),
                  if (_previewLoading)
                    const Row(
                      children: <Widget>[
                        SizedBox(
                          width: 14,
                          height: 14,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        ),
                        SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            'Requesting server advisory preview...',
                            style: TextStyle(fontSize: 11.5),
                          ),
                        ),
                      ],
                    )
                  else if (_previewError != null)
                    Text(
                      _previewError!,
                      style: TextStyle(fontSize: 11.5, color: palette.danger),
                    )
                  else if (_preview != null) ...<Widget>[
                    Wrap(
                      spacing: 8,
                      runSpacing: 6,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: <Widget>[
                        if (_preview!.eligible)
                          SemanticBadge(
                            label: 'Eligible (advisory)',
                            foreground: palette.ok,
                            background: palette.okContainer,
                            dotColor: palette.ok,
                            outlined: true,
                          )
                        else
                          SemanticBadge(
                            label: 'Ineligible (blocked)',
                            foreground: palette.danger,
                            background: palette.dangerContainer,
                            dotColor: palette.danger,
                            outlined: true,
                          ),
                        if (_preview!.observedCarAvailable &&
                            _preview!.observedFloorReference != null)
                          Text(
                            'Observed: ${_preview!.observedFloorReference}F · ${_preview!.observedOperatingMode?.wireValue ?? "unknown"}',
                            style: instrumentMonoStyle(
                              context,
                              fontSize: 11,
                              color: palette.subtleText,
                            ),
                          )
                        else
                          Text(
                            'Observed Operational state unavailable',
                            style: TextStyle(
                              fontSize: 11,
                              color: palette.caution,
                            ),
                          ),
                      ],
                    ),
                    if (_preview!.blocker != null) ...<Widget>[
                      const SizedBox(height: 6),
                      Text(
                        'Blocker (${_preview!.blocker!.code}): ${_preview!.blocker!.detail}',
                        key: const Key('serve-floor-preview-blocker'),
                        style: TextStyle(
                          fontSize: 11.5,
                          height: 1.35,
                          color: palette.danger,
                        ),
                      ),
                    ],
                  ],
                ],
              ),
            ),
          ),
          const SizedBox(height: 12),
        ],

        if (!state.isSnapshotLive)
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: Text(
              'Host connection is not live; submission is disabled until reconnected.',
              style: TextStyle(fontSize: 11.5, color: palette.caution),
            ),
          ),

        Wrap(
          alignment: WrapAlignment.end,
          spacing: 8,
          runSpacing: 8,
          children: <Widget>[
            OutlinedButton(
              onPressed: widget.onClose,
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: const Key('serve-floor-submit-button'),
              onPressed: canSubmit
                  ? () => unawaited(_submitTransaction())
                  : null,
              child: Text(
                _selectedFloor != null
                    ? 'Submit ServeFloor (${widget.carId} → ${_selectedFloor}F)'
                    : 'Submit ServeFloor',
              ),
            ),
          ],
        ),
      ],
    );
  }

  Widget _buildStatusView(
    BuildContext context,
    ClientState state,
    TrackedServeFloorOperation operation,
    OperationalCar? liveCar,
  ) {
    final InstrumentPalette palette = context.palette;
    final (
      String statusLabel,
      Color fg,
      Color bg,
      Color dotColor,
      bool hollowDot,
    ) = describeProjectedOperationStatus(
      operation,
      palette,
    );
    final ProjectedSubmission? proj = operation.projection;

    return Column(
      key: const Key('serve-floor-status-panel'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        Wrap(
          spacing: 8,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            SemanticBadge(
              label: statusLabel,
              foreground: fg,
              background: bg,
              dotColor: dotColor,
              hollowDot: hollowDot,
              outlined: true,
            ),
            if (proj?.intentId != null)
              Text(
                'IntentId #${proj!.intentId}',
                style: instrumentMonoStyle(
                  context,
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                ),
              )
            else
              Text(
                'IntentId: none',
                style: instrumentMonoStyle(
                  context,
                  fontSize: 11.5,
                  color: palette.mutedText,
                ),
              ),
          ],
        ),
        const SizedBox(height: 8),
        Text(
          describeOperationTruthCaption(operation),
          style: TextStyle(
            fontSize: 12,
            height: 1.4,
            color: Theme.of(context).colorScheme.onSurface,
          ),
        ),
        const SizedBox(height: 10),
        DecoratedBox(
          decoration: BoxDecoration(
            color: palette.elevatedSurface,
            borderRadius: BorderRadius.circular(5),
            border: Border.all(color: palette.border),
          ),
          child: Padding(
            padding: const EdgeInsets.all(10),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                _KvLine(
                  label: 'Transaction',
                  value: 'ServeFloor (${operation.car} → ${operation.floor}F)',
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Correlation key',
                  value: operation.submissionKey,
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Engine IntentId',
                  value: proj?.intentId != null ? '#${proj!.intentId}' : 'none',
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Engine receipt',
                  value: proj?.receipt ?? 'unavailable',
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Reconciliation',
                  value: proj?.reconciliation ?? 'unavailable',
                  monospace: true,
                ),
                const SizedBox(height: 3),
                _KvLine(
                  label: 'Physical progress',
                  value: proj == null
                      ? 'unknown'
                      : (proj.sawProgress ? 'observed' : 'not observed yet'),
                  monospace: true,
                ),
                if (operation.effectiveKnowledgeReason != null) ...<Widget>[
                  const SizedBox(height: 3),
                  _KvLine(
                    label: 'Knowledge detail',
                    value: operation.effectiveKnowledgeReason!,
                    monospace: true,
                  ),
                ],
                if (liveCar != null) ...<Widget>[
                  const SizedBox(height: 3),
                  _KvLine(
                    label: 'Live car location',
                    value: switch (liveCar.location) {
                      FloorLocation(:final floor) => 'Floor $floor',
                      ContinuousLocation(:final floorPosition) =>
                        '${floorPosition.toStringAsFixed(2)}F',
                    },
                    monospace: true,
                  ),
                ],
              ],
            ),
          ),
        ),
        if (operation.lastQueryError != null) ...<Widget>[
          const SizedBox(height: 6),
          Text(
            operation.lastQueryError!,
            style: TextStyle(fontSize: 11.5, color: palette.caution),
          ),
        ],
        const SizedBox(height: 12),
        Wrap(
          alignment: WrapAlignment.end,
          spacing: 8,
          runSpacing: 8,
          children: <Widget>[
            OutlinedButton(
              key: const Key('modal-query-status-button'),
              onPressed:
                  state.isSnapshotLive &&
                      !operation.submitInFlight &&
                      !operation.queryInFlight
                  ? () => unawaited(widget.controller.queryTrackedSubmission())
                  : null,
              child: Text(
                operation.queryInFlight ? 'Querying...' : 'Query status',
              ),
            ),
            if (liveCar != null && !operation.submitInFlight)
              OutlinedButton(
                key: const Key('modal-new-attempt-button'),
                onPressed: _startNewAttempt,
                child: const Text('New attempt'),
              ),
            FilledButton(onPressed: widget.onClose, child: const Text('Close')),
          ],
        ),
      ],
    );
  }
}

class _KvLine extends StatelessWidget {
  const _KvLine({
    required this.label,
    required this.value,
    this.monospace = false,
  });

  final String label;
  final String value;
  final bool monospace;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        SizedBox(
          width: 118,
          child: Text(
            label,
            style: TextStyle(fontSize: 11, color: palette.mutedText),
          ),
        ),
        const SizedBox(width: 6),
        Expanded(
          child: Text(
            value,
            style: monospace
                ? instrumentMonoStyle(
                    context,
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                  )
                : const TextStyle(fontSize: 11.5, fontWeight: FontWeight.w500),
          ),
        ),
      ],
    );
  }
}
