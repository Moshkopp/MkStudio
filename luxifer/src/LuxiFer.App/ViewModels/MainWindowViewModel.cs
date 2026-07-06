using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using LuxiFer.App.Controls;
using LuxiFer.Core.Canvas;
using LuxiFer.Core.Projects;
using LuxiFer.Core.Undo;

namespace LuxiFer.App.ViewModels;

public partial class MainWindowViewModel : ViewModelBase
{
    [ObservableProperty]
    private Project _project;

    /// <summary>Undo-/Redo-Historie; alle Canvas-Aktionen laufen hierüber.</summary>
    public UndoStack Undo { get; } = new();

    [ObservableProperty]
    private CanvasTool _activeTool = CanvasTool.Select;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsDesignMode))]
    [NotifyPropertyChangedFor(nameof(IsLaserMode))]
    [NotifyPropertyChangedFor(nameof(ShowNoSelectionHint))]
    private WorkMode _mode = WorkMode.Design;

    public bool IsDesignMode => Mode == WorkMode.Design;
    public bool IsLaserMode => Mode == WorkMode.Laser;

    /// <summary>Hinweis „kein Objekt" nur im Design-Modus ohne Auswahl.</summary>
    public bool ShowNoSelectionHint => IsDesignMode && !HasSelection;

    [RelayCommand]
    private void SetMode(WorkMode mode)
    {
        Mode = mode;
        StatusText = mode == WorkMode.Design
            ? "Design-Modus: Zeichnen und Anordnen"
            : "Laser-Modus: Maschinenparameter und Job";
    }

    [ObservableProperty]
    private Layer? _activeLayer;

    [ObservableProperty]
    private string _statusText = "Bereit";

    [ObservableProperty]
    private string _cursorPosition = "";

    public ObservableCollection<Layer> Layers { get; } = [];

    public static LayerMode[] LayerModes { get; } = Enum.GetValues<LayerMode>();

    /// <summary>Das Canvas soll neu gezeichnet werden (Parameter im Panel geändert).</summary>
    public event EventHandler? CanvasInvalidateRequested;

    public MainWindowViewModel()
    {
        _project = NewProjectInternal();
        SyncLayers();
        Undo.Changed += (_, _) =>
        {
            UndoActionCommand.NotifyCanExecuteChanged();
            RedoActionCommand.NotifyCanExecuteChanged();
            OnPropertyChanged(nameof(UndoHint));
            OnPropertyChanged(nameof(RedoHint));
            RefreshSelectionFields();
            CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
        };
    }

    public string Title => $"LuxiFer — {Project.Name}";

    public string UndoHint => Undo.NextUndoLabel is { } l ? $"Rückgängig: {l}" : "Rückgängig";
    public string RedoHint => Undo.NextRedoLabel is { } l ? $"Wiederholen: {l}" : "Wiederholen";

    [RelayCommand(CanExecute = nameof(CanUndo))]
    private void UndoAction()
    {
        Undo.Undo();
        StatusText = "Rückgängig";
    }

    private bool CanUndo() => Undo.CanUndo;

    [RelayCommand(CanExecute = nameof(CanRedo))]
    private void RedoAction()
    {
        Undo.Redo();
        StatusText = "Wiederholt";
    }

    private bool CanRedo() => Undo.CanRedo;

    private static Project NewProjectInternal()
    {
        var project = new Project { Name = "Unbenannt" };
        project.Canvas.Layers.Add(Layer.CreateNext(0));
        return project;
    }

    private void SyncLayers()
    {
        Layers.Clear();
        foreach (var layer in Project.Canvas.Layers)
            Layers.Add(layer);
        ActiveLayer = Layers.FirstOrDefault();
        OnPropertyChanged(nameof(Title));
    }

    [RelayCommand]
    private void NewProject()
    {
        Project = NewProjectInternal();
        SelectedObject = null;
        Undo.Clear();
        SyncLayers();
        StatusText = "Neues Projekt angelegt";
    }

    [RelayCommand]
    private void AddLayer()
    {
        var layer = Layer.CreateNext(Project.Canvas.Layers.Count);
        Project.Canvas.Layers.Add(layer);
        Layers.Add(layer);
        ActiveLayer = layer;
        StatusText = $"{layer.Name} hinzugefügt";
    }

    /// <summary>
    /// Weist einem Layer eine Palettenfarbe zu. Da <see cref="Layer"/> ein
    /// reines Core-POCO ohne Change-Notification ist, wird die Zeile über einen
    /// Replace derselben Instanz neu gebunden; die Auswahl bleibt erhalten.
    /// </summary>
    [RelayCommand]
    private void SetLayerColor((Layer Layer, string Color) arg)
    {
        arg.Layer.ColorHex = arg.Color;
        var index = Layers.IndexOf(arg.Layer);
        if (index >= 0) Layers[index] = arg.Layer;
        ActiveLayer = arg.Layer;
        CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>
    /// Farbpalette im Design-Modus (ADR 0005): mit Auswahl werden die
    /// <b>selektierten Objekte</b> umgefärbt (ein Undo-Schritt); ohne Auswahl
    /// wird die Vorgabefarbe des aktiven Layers gesetzt (für neue Objekte).
    /// </summary>
    [RelayCommand]
    private void SetActiveColor(string color)
    {
        if (SelectedObjects.Count > 0)
        {
            Undo.Execute(new RecolorObjectsCommand(SelectedObjects.ToArray(), color));
            Project.ModifiedAt = DateTimeOffset.UtcNow;
            CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
            StatusText = SelectedObjects.Count == 1
                ? $"Objektfarbe {color}"
                : $"{SelectedObjects.Count} Objekte gefärbt";
            return;
        }
        if (ActiveLayer is null) return;
        SetLayerColor((ActiveLayer, color));
        StatusText = $"Vorgabefarbe {color}";
    }

    [RelayCommand]
    private void RemoveLayer(Layer? layer)
    {
        if (layer is null || Project.Canvas.Layers.Count <= 1) return;
        Project.Canvas.Layers.Remove(layer);
        Layers.Remove(layer);
        if (ActiveLayer == layer) ActiveLayer = Layers.FirstOrDefault();
        StatusText = $"{layer.Name} entfernt";
        CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
    }

    [RelayCommand]
    private void SelectTool(CanvasTool tool)
    {
        ActiveTool = tool;
        StatusText = tool switch
        {
            CanvasTool.Select => "Auswählen: Klicken wählt, Ziehen verschiebt, Handles skalieren",
            CanvasTool.Rectangle => "Rechteck aufziehen",
            CanvasTool.Ellipse => "Ellipse aufziehen",
            CanvasTool.Line => "Linie ziehen",
            CanvasTool.Polyline => "Polyline: Klick setzt Punkte, Enter/Doppelklick beendet, Esc bricht ab",
            CanvasTool.Polygon => "Polygon: Klick setzt Punkte, Enter/Doppelklick schließt, Esc bricht ab",
            _ => "",
        };
    }

    public void ReportCursor(double xMm, double yMm) =>
        CursorPosition = $"X {xMm:F1} mm   Y {yMm:F1} mm";

    public void MarkDirty()
    {
        Project.ModifiedAt = DateTimeOffset.UtcNow;
        StatusText = "Geändert";
        RefreshSelectionFields();
    }

    // ----- Auswahl / Eigenschaften-Panel -----

    /// <summary>
    /// Aktuelle Auswahl als Menge (ADR 0004). Vom Canvas gepflegt. Das
    /// „primäre" Objekt <see cref="SelectedObject"/> ist das zuletzt
    /// hinzugefügte und speist die Transform-Palette bei Einzelauswahl.
    /// </summary>
    public ObservableCollection<CanvasObject> SelectedObjects { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelection))]
    [NotifyPropertyChangedFor(nameof(ShowNoSelectionHint))]
    [NotifyPropertyChangedFor(nameof(IsSingleSelection))]
    private CanvasObject? _selectedObject;

    public bool HasSelection => SelectedObjects.Count > 0;
    public bool IsSingleSelection => SelectedObjects.Count == 1;

    /// <summary>Ausrichten braucht ≥2, Verteilen ≥3 Objekte (ADR 0004 §5).</summary>
    public bool CanAlign => SelectedObjects.Count >= 2;
    public bool CanDistribute => SelectedObjects.Count >= 3;

    /// <summary>Vom Canvas nach jeder Auswahländerung aufzurufen.</summary>
    public void OnSelectionChanged()
    {
        OnPropertyChanged(nameof(HasSelection));
        OnPropertyChanged(nameof(IsSingleSelection));
        OnPropertyChanged(nameof(ShowNoSelectionHint));
        OnPropertyChanged(nameof(CanAlign));
        OnPropertyChanged(nameof(CanDistribute));
        AlignLeftCommand.NotifyCanExecuteChanged();
        AlignHCenterCommand.NotifyCanExecuteChanged();
        AlignRightCommand.NotifyCanExecuteChanged();
        AlignTopCommand.NotifyCanExecuteChanged();
        AlignVCenterCommand.NotifyCanExecuteChanged();
        AlignBottomCommand.NotifyCanExecuteChanged();
        DistributeHCommand.NotifyCanExecuteChanged();
        DistributeVCommand.NotifyCanExecuteChanged();
        RefreshSelectionFields();
    }

    [ObservableProperty] private double _selX;
    [ObservableProperty] private double _selY;
    [ObservableProperty] private double _selWidth;
    [ObservableProperty] private double _selHeight;
    [ObservableProperty] private double _selRotation;
    [ObservableProperty] private double _selScalePct = 100;

    /// <summary>Seitenverhältnis von Breite/Höhe sperren.</summary>
    [ObservableProperty] private bool _lockAspect;

    private bool _updatingSelectionFields;

    partial void OnSelectedObjectChanged(CanvasObject? value) => RefreshSelectionFields();

    private void RefreshSelectionFields()
    {
        if (SelectedObjects.Count == 0) return;
        _updatingSelectionFields = true;
        if (SelectedObjects.Count == 1)
        {
            var (x, y, w, h) = SelectedObjects[0].Bounds;
            SelX = Math.Round(x, 2);
            SelY = Math.Round(y, 2);
            SelWidth = Math.Round(w, 2);
            SelHeight = Math.Round(h, 2);
            SelRotation = Math.Round(SelectedObjects[0].Rotation, 1);
        }
        else
        {
            // Mehrfachauswahl: gemeinsame Bounding-Box; nur X/Y sind aktiv.
            var (x, y, w, h) = Arrange.GroupBounds(SelectedObjects.ToArray());
            SelX = Math.Round(x, 2);
            SelY = Math.Round(y, 2);
            SelWidth = Math.Round(w, 2);
            SelHeight = Math.Round(h, 2);
            SelRotation = 0;
        }
        SelScalePct = 100;
        _updatingSelectionFields = false;
    }

    // Bounds/Rotation vor Beginn einer Panel-Bearbeitung, für je ein Undo-Command
    // über die gesamte Feld-Editiersequenz (bis CommitSelectionEdit).
    private (double X, double Y, double W, double H)? _editStartBounds;
    private double? _editStartRotation;

    partial void OnSelXChanged(double value) => ApplySelectionBounds();
    partial void OnSelYChanged(double value) => ApplySelectionBounds();

    partial void OnSelWidthChanged(double value)
    {
        if (!IsSingleSelection) return; // Größe nur bei Einzelauswahl
        if (_updatingSelectionFields || !LockAspect) { ApplySelectionBounds(); return; }
        // Seitenverhältnis halten: Höhe proportional zur alten Breite nachziehen.
        var start = _editStartBounds ?? SelectedObject?.Bounds;
        if (start is { W: > 0 } s)
        {
            _updatingSelectionFields = true;
            SelHeight = Math.Round(Math.Max(0.1, value) * s.H / s.W, 2);
            _updatingSelectionFields = false;
        }
        ApplySelectionBounds();
    }

    partial void OnSelHeightChanged(double value)
    {
        if (!IsSingleSelection) return; // Größe nur bei Einzelauswahl
        if (_updatingSelectionFields || !LockAspect) { ApplySelectionBounds(); return; }
        var start = _editStartBounds ?? SelectedObject?.Bounds;
        if (start is { H: > 0 } s)
        {
            _updatingSelectionFields = true;
            SelWidth = Math.Round(Math.Max(0.1, value) * s.W / s.H, 2);
            _updatingSelectionFields = false;
        }
        ApplySelectionBounds();
    }

    partial void OnSelRotationChanged(double value)
    {
        if (_updatingSelectionFields || SelectedObject is null) return;
        _editStartRotation ??= SelectedObject.Rotation;
        SelectedObject.Rotation = value;
        Project.ModifiedAt = DateTimeOffset.UtcNow;
        CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
    }

    partial void OnSelScalePctChanged(double value)
    {
        if (_updatingSelectionFields || SelectedObject is null || value <= 0) return;
        // Skaliert relativ zu den Bounds bei Editierbeginn, um den Mittelpunkt.
        _editStartBounds ??= SelectedObject.Bounds;
        var s = _editStartBounds.Value;
        var f = value / 100.0;
        var nw = Math.Max(0.1, s.W * f);
        var nh = Math.Max(0.1, s.H * f);
        var nx = s.X + (s.W - nw) / 2;
        var ny = s.Y + (s.H - nh) / 2;
        SelectedObject.SetBounds(nx, ny, nw, nh);
        RefreshBoundsFields();
        Project.ModifiedAt = DateTimeOffset.UtcNow;
        CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
    }

    private void ApplySelectionBounds()
    {
        if (_updatingSelectionFields || SelectedObjects.Count == 0) return;

        if (IsSingleSelection)
        {
            _editStartBounds ??= SelectedObject!.Bounds;
            SelectedObject!.SetBounds(SelX, SelY, Math.Max(0.1, SelWidth), Math.Max(0.1, SelHeight));
        }
        else
        {
            // Mehrfachauswahl: X/Y verschieben die gesamte Gruppe um das Delta
            // zwischen aktueller und eingegebener Gruppen-Position.
            var (gx, gy, _, _) = Arrange.GroupBounds(SelectedObjects.ToArray());
            var dx = SelX - gx;
            var dy = SelY - gy;
            if (dx == 0 && dy == 0) return;
            _editStartGroupDelta = (_editStartGroupDelta.Dx + dx, _editStartGroupDelta.Dy + dy);
            foreach (var o in SelectedObjects) o.MoveBy(dx, dy);
        }
        Project.ModifiedAt = DateTimeOffset.UtcNow;
        CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
    }

    // Aufsummiertes Gruppen-Verschiebe-Delta während einer Panel-Editiersequenz.
    private (double Dx, double Dy) _editStartGroupDelta;

    // Aktualisiert nur die X/Y/B/H-Felder aus dem Objekt (z. B. nach Skalierung).
    private void RefreshBoundsFields()
    {
        if (SelectedObject is null) return;
        _updatingSelectionFields = true;
        var (x, y, w, h) = SelectedObject.Bounds;
        SelX = Math.Round(x, 2);
        SelY = Math.Round(y, 2);
        SelWidth = Math.Round(w, 2);
        SelHeight = Math.Round(h, 2);
        _updatingSelectionFields = false;
    }

    /// <summary>
    /// Schließt eine Feld-Bearbeitung ab (Enter/Fokusverlust) und legt die
    /// Änderung(en) als Undo-Command(s) ab. Von der View aufgerufen.
    /// </summary>
    public void CommitSelectionEdit()
    {
        // Gruppen-Verschiebung über die Panel-Felder als ein Undo-Schritt.
        if (!IsSingleSelection)
        {
            if (_editStartGroupDelta is { Dx: not 0 } or { Dy: not 0 })
            {
                var objs = SelectedObjects.ToArray();
                var deltas = objs.Select(_ => _editStartGroupDelta).ToArray();
                Undo.Push(new ArrangeObjectsCommand(objs, deltas, "Verschieben"));
            }
            _editStartGroupDelta = (0, 0);
            return;
        }

        if (SelectedObject is null)
        {
            _editStartBounds = null;
            _editStartRotation = null;
            return;
        }

        if (_editStartBounds is { } before)
        {
            var after = SelectedObject.Bounds;
            if (after != before)
                Undo.Push(new ResizeObjectCommand(SelectedObject, before, after));
        }
        if (_editStartRotation is { } beforeRot && beforeRot != SelectedObject.Rotation)
            Undo.Push(new RotateObjectCommand(SelectedObject, beforeRot, SelectedObject.Rotation));

        _editStartBounds = null;
        _editStartRotation = null;
        _updatingSelectionFields = true;
        SelScalePct = 100;
        _updatingSelectionFields = false;
    }

    // ----- Anordnen (Ausrichten / Verteilen) über die Auswahl (ADR 0004 §5) -----

    private void ApplyArrange(
        IReadOnlyList<(double Dx, double Dy)> deltas, string label)
    {
        if (deltas.All(d => d is { Dx: 0, Dy: 0 })) return;
        var objects = SelectedObjects.ToArray();
        Undo.Execute(new ArrangeObjectsCommand(objects, deltas, label));
        Project.ModifiedAt = DateTimeOffset.UtcNow;
        RefreshSelectionFields();
        CanvasInvalidateRequested?.Invoke(this, EventArgs.Empty);
        StatusText = label;
    }

    private void DoAlign(AlignKind kind, string label) =>
        ApplyArrange(Arrange.Align(SelectedObjects.ToArray(), kind), label);

    private void DoDistribute(DistributeKind kind, string label) =>
        ApplyArrange(Arrange.Distribute(SelectedObjects.ToArray(), kind), label);

    [RelayCommand(CanExecute = nameof(CanAlign))]
    private void AlignLeft() => DoAlign(AlignKind.Left, "Links ausrichten");
    [RelayCommand(CanExecute = nameof(CanAlign))]
    private void AlignHCenter() => DoAlign(AlignKind.HCenter, "Horizontal zentrieren");
    [RelayCommand(CanExecute = nameof(CanAlign))]
    private void AlignRight() => DoAlign(AlignKind.Right, "Rechts ausrichten");
    [RelayCommand(CanExecute = nameof(CanAlign))]
    private void AlignTop() => DoAlign(AlignKind.Top, "Oben ausrichten");
    [RelayCommand(CanExecute = nameof(CanAlign))]
    private void AlignVCenter() => DoAlign(AlignKind.VCenter, "Vertikal zentrieren");
    [RelayCommand(CanExecute = nameof(CanAlign))]
    private void AlignBottom() => DoAlign(AlignKind.Bottom, "Unten ausrichten");

    [RelayCommand(CanExecute = nameof(CanDistribute))]
    private void DistributeH() => DoDistribute(DistributeKind.Horizontal, "Horizontal verteilen");
    [RelayCommand(CanExecute = nameof(CanDistribute))]
    private void DistributeV() => DoDistribute(DistributeKind.Vertical, "Vertikal verteilen");
}
