using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Data;
using Avalonia.Input;
using Avalonia.Media;
using LuxiFer.Core.Canvas;
using LuxiFer.Core.Undo;

namespace LuxiFer.App.Controls;

public enum CanvasTool
{
    Select,
    Rectangle,
    Ellipse,
    Line,
    Polyline,
    Polygon,
}

/// <summary>Welche Kante(n) ein Skalier-Handle bewegt (N=oben, S=unten, W=links, E=rechts).</summary>
internal enum ResizeHandle
{
    N, S, W, E, NW, NE, SW, SE,
}

/// <summary>
/// Rendert das CanvasDocument (mm-Koordinaten) und übersetzt Eingaben
/// in Änderungen am Dokument.
/// Zoom: Mausrad · Pan: mittlere Taste · Auswahl/Verschieben/Skalieren: linke Taste.
/// Polyline/Polygon: Klick setzt Punkte, Doppelklick/Enter schließt ab, Esc bricht ab.
/// </summary>
public sealed class CanvasControl : Control
{
    public static readonly StyledProperty<CanvasDocument?> DocumentProperty =
        AvaloniaProperty.Register<CanvasControl, CanvasDocument?>(nameof(Document));

    public static readonly StyledProperty<CanvasTool> ToolProperty =
        AvaloniaProperty.Register<CanvasControl, CanvasTool>(nameof(Tool));

    public static readonly StyledProperty<Layer?> ActiveLayerProperty =
        AvaloniaProperty.Register<CanvasControl, Layer?>(nameof(ActiveLayer));

    public static readonly DirectProperty<CanvasControl, CanvasObject?> SelectedObjectProperty =
        AvaloniaProperty.RegisterDirect<CanvasControl, CanvasObject?>(
            nameof(SelectedObject), o => o.SelectedObject, (o, v) => o.SelectedObject = v,
            defaultBindingMode: BindingMode.TwoWay);

    public CanvasDocument? Document
    {
        get => GetValue(DocumentProperty);
        set => SetValue(DocumentProperty, value);
    }

    public CanvasTool Tool
    {
        get => GetValue(ToolProperty);
        set => SetValue(ToolProperty, value);
    }

    public Layer? ActiveLayer
    {
        get => GetValue(ActiveLayerProperty);
        set => SetValue(ActiveLayerProperty, value);
    }

    public static readonly StyledProperty<UndoStack?> UndoStackProperty =
        AvaloniaProperty.Register<CanvasControl, UndoStack?>(nameof(UndoStack));

    /// <summary>Historie, in die alle Canvas-Aktionen als Commands laufen.</summary>
    public UndoStack? UndoStack
    {
        get => GetValue(UndoStackProperty);
        set => SetValue(UndoStackProperty, value);
    }

    private CanvasObject? _selectedObject;
    public CanvasObject? SelectedObject
    {
        get => _selectedObject;
        set
        {
            SetAndRaise(SelectedObjectProperty, ref _selectedObject, value);
            InvalidateVisual();
        }
    }

    /// <summary>Mausposition in mm, für die Statusleiste.</summary>
    public event EventHandler<Point>? PointerMillimeterMoved;

    /// <summary>Dokument wurde durch eine Benutzeraktion geändert.</summary>
    public event EventHandler? DocumentChanged;

    /// <summary>Zoom oder Pan hat sich geändert (für synchronisierte Lineale).</summary>
    public event EventHandler? ViewChanged;

    private double _zoomBacking = 1.0;             // Pixel pro mm
    private Point _panOffsetBacking = new(40, 40); // Pixel

    private double _zoom
    {
        get => _zoomBacking;
        set
        {
            if (_zoomBacking == value) return;
            _zoomBacking = value;
            ViewChanged?.Invoke(this, EventArgs.Empty);
        }
    }

    private Point _panOffset
    {
        get => _panOffsetBacking;
        set
        {
            if (_panOffsetBacking == value) return;
            _panOffsetBacking = value;
            ViewChanged?.Invoke(this, EventArgs.Empty);
        }
    }

    /// <summary>Aktueller Zoom in Pixel pro mm.</summary>
    public double ZoomPxPerMm => _zoomBacking;

    /// <summary>Pan-Versatz in Pixeln (Bildschirmposition des mm-Nullpunkts).</summary>
    public Point PanOffset => _panOffsetBacking;

    private bool _panning;
    private Point _panStart, _panOffsetStart;
    private bool _userAdjustedView;         // hat der Nutzer selbst gezoomt/gepannt?

    private CanvasObject? _drawingObject;   // Rect/Ellipse/Linie im Aufziehen
    private Layer? _drawingLayer;           // Layer, in dem gerade gezeichnet wird
    private PolylineObject? _polyInProgress;
    private Point _polyPreviewMm;
    private Point _dragStartMm;
    private bool _draggingSelection;
    private Point _moveStartMm;             // Startpunkt eines Verschiebe-Drags

    private ResizeHandle? _activeHandle;
    private (double X, double Y, double W, double H) _resizeStartBounds;

    private const double HandleSizePx = 8;

    static CanvasControl()
    {
        AffectsRender<CanvasControl>(DocumentProperty, ToolProperty, ActiveLayerProperty);
    }

    public CanvasControl()
    {
        Focusable = true;
        ClipToBounds = true;
    }

    private Point ToMm(Point screen) =>
        new((screen.X - _panOffset.X) / _zoom, (screen.Y - _panOffset.Y) / _zoom);

    private Point ToScreen(double xMm, double yMm) =>
        new(xMm * _zoom + _panOffset.X, yMm * _zoom + _panOffset.Y);

    /// <summary>
    /// Freizuhaltende Ränder (Pixel), damit das Bett beim Einpassen nicht unter
    /// den schwebenden Panelen landet. Von der View gesetzt.
    /// </summary>
    public Thickness ContentInset { get; set; } = new(48);

    public void ZoomToFit()
    {
        if (Document is null || Bounds.Width <= 0) return;
        var availW = Bounds.Width - ContentInset.Left - ContentInset.Right;
        var availH = Bounds.Height - ContentInset.Top - ContentInset.Bottom;
        if (availW <= 0 || availH <= 0) return;

        _zoom = Math.Max(0.01, Math.Min(availW / Document.WidthMm, availH / Document.HeightMm));
        // Bett im freien Bereich zwischen den Insets zentrieren
        _panOffset = new Point(
            ContentInset.Left + (availW - Document.WidthMm * _zoom) / 2,
            ContentInset.Top + (availH - Document.HeightMm * _zoom) / 2);
        _userAdjustedView = false; // Einpassen ist der automatische Zustand
        InvalidateVisual();
    }

    protected override void OnSizeChanged(SizeChangedEventArgs e)
    {
        base.OnSizeChanged(e);
        // Automatisch einpassen, solange der Nutzer die Ansicht nicht selbst
        // per Zoom/Pan verändert hat (z. B. auch beim Maximieren des Fensters).
        if (!_userAdjustedView) ZoomToFit();
    }

    protected override void OnPointerWheelChanged(PointerWheelEventArgs e)
    {
        var pos = e.GetPosition(this);
        var mmBefore = ToMm(pos);
        var factor = e.Delta.Y > 0 ? 1.2 : 1 / 1.2;
        _zoom = Math.Clamp(_zoom * factor, 0.05, 100);
        // Punkt unter dem Cursor festhalten
        _panOffset = new Point(pos.X - mmBefore.X * _zoom, pos.Y - mmBefore.Y * _zoom);
        _userAdjustedView = true;
        InvalidateVisual();
        e.Handled = true;
    }

    protected override void OnPointerPressed(PointerPressedEventArgs e)
    {
        Focus();
        var point = e.GetCurrentPoint(this);
        var pos = point.Position;

        if (point.Properties.IsMiddleButtonPressed)
        {
            _panning = true;
            _panStart = pos;
            _panOffsetStart = _panOffset;
            _userAdjustedView = true;
            e.Handled = true;
            return;
        }

        if (!point.Properties.IsLeftButtonPressed || Document is null) return;
        var mm = ToMm(pos);
        var canDraw = ActiveLayer is { Locked: false };

        switch (Tool)
        {
            case CanvasTool.Select:
                // Zuerst prüfen, ob ein Resize-Handle getroffen wurde
                if (SelectedObject is not null && HitHandle(pos) is { } handle)
                {
                    _activeHandle = handle;
                    _resizeStartBounds = SelectedObject.Bounds;
                    _dragStartMm = mm;
                    break;
                }
                SelectedObject = Document.HitTest(mm.X, mm.Y, tolerance: 2 / _zoom);
                if (SelectedObject is not null)
                {
                    _draggingSelection = true;
                    _dragStartMm = mm;
                    _moveStartMm = mm;
                }
                break;

            case CanvasTool.Rectangle when canDraw:
                _drawingObject = new RectangleObject { X = mm.X, Y = mm.Y };
                break;
            case CanvasTool.Ellipse when canDraw:
                _drawingObject = new EllipseObject { X = mm.X, Y = mm.Y };
                break;
            case CanvasTool.Line when canDraw:
                _drawingObject = new LineObject { X = mm.X, Y = mm.Y, X2 = mm.X, Y2 = mm.Y };
                break;

            case CanvasTool.Polyline or CanvasTool.Polygon when canDraw:
                if (e.ClickCount >= 2)
                {
                    FinishPolyline();
                    break;
                }
                if (_polyInProgress is null)
                {
                    _polyInProgress = new PolylineObject { Closed = Tool == CanvasTool.Polygon };
                    ActiveLayer!.Objects.Add(_polyInProgress);
                }
                _polyInProgress.Points.Add((mm.X, mm.Y));
                _polyPreviewMm = mm;
                InvalidateVisual();
                break;
        }

        if (_drawingObject is not null)
        {
            _dragStartMm = mm;
            _drawingLayer = ActiveLayer;
            // Live in den Layer, damit man beim Aufziehen sieht was entsteht;
            // beim Loslassen wird daraus ein Undo-Command (siehe OnPointerReleased).
            ActiveLayer!.Objects.Add(_drawingObject);
            InvalidateVisual();
        }
        e.Handled = true;
    }

    protected override void OnPointerMoved(PointerEventArgs e)
    {
        var pos = e.GetPosition(this);
        var mm = ToMm(pos);
        PointerMillimeterMoved?.Invoke(this, mm);

        if (_panning)
        {
            _panOffset = _panOffsetStart + (pos - _panStart);
            InvalidateVisual();
            return;
        }

        UpdateCursor(pos);

        if (_activeHandle is { } handle && SelectedObject is not null)
        {
            ApplyResize(handle, mm);
            InvalidateVisual();
            return;
        }

        if (_polyInProgress is not null)
        {
            _polyPreviewMm = mm;
            InvalidateVisual();
            return;
        }

        if (_drawingObject is not null)
        {
            switch (_drawingObject)
            {
                case RectangleObject r:
                    r.SetBounds(
                        Math.Min(_dragStartMm.X, mm.X), Math.Min(_dragStartMm.Y, mm.Y),
                        Math.Abs(mm.X - _dragStartMm.X), Math.Abs(mm.Y - _dragStartMm.Y));
                    break;
                case EllipseObject el:
                    el.SetBounds(
                        Math.Min(_dragStartMm.X, mm.X), Math.Min(_dragStartMm.Y, mm.Y),
                        Math.Abs(mm.X - _dragStartMm.X), Math.Abs(mm.Y - _dragStartMm.Y));
                    break;
                case LineObject line:
                    line.X2 = mm.X;
                    line.Y2 = mm.Y;
                    break;
            }
            InvalidateVisual();
            return;
        }

        if (_draggingSelection && SelectedObject is not null)
        {
            SelectedObject.MoveBy(mm.X - _dragStartMm.X, mm.Y - _dragStartMm.Y);
            _dragStartMm = mm;
            InvalidateVisual();
        }
    }

    protected override void OnPointerReleased(PointerReleasedEventArgs e)
    {
        _panning = false;

        if (_activeHandle is not null && SelectedObject is not null)
        {
            _activeHandle = null;
            var after = SelectedObject.Bounds;
            if (after != _resizeStartBounds)
            {
                UndoStack?.Push(new ResizeObjectCommand(SelectedObject, _resizeStartBounds, after));
                RaiseDocumentChanged();
            }
            return;
        }

        if (_drawingObject is not null)
        {
            // Degenerierte Objekte (bloßer Klick) wieder entfernen
            var (_, _, w, h) = _drawingObject.Bounds;
            if (w < 0.1 && h < 0.1)
                _drawingLayer?.Objects.Remove(_drawingObject);
            else
                RegisterAdd(_drawingObject, _drawingLayer);
            _drawingObject = null;
            _drawingLayer = null;
            InvalidateVisual();
        }

        if (_draggingSelection && SelectedObject is not null)
        {
            _draggingSelection = false;
            var dx = _dragStartMm.X - _moveStartMm.X;
            var dy = _dragStartMm.Y - _moveStartMm.Y;
            if (dx != 0 || dy != 0)
            {
                // Verschiebung ist bereits live erfolgt → nur als Command ablegen.
                UndoStack?.Push(new MoveObjectCommand(SelectedObject, dx, dy));
                RaiseDocumentChanged();
            }
        }
    }

    /// <summary>
    /// Überführt ein interaktiv bereits eingefügtes Objekt in ein
    /// AddObjectCommand: einmal entfernen, dann via Execute sauber hinzufügen.
    /// So sind Do/Undo konsistent und das Objekt bleibt selektiert.
    /// </summary>
    private void RegisterAdd(CanvasObject obj, Layer? layer)
    {
        layer ??= ActiveLayer;
        if (layer is null) return;
        layer.Objects.Remove(obj);
        if (UndoStack is not null)
            UndoStack.Execute(new AddObjectCommand(layer, obj));
        else
            layer.Objects.Add(obj);
        SelectedObject = obj;
        RaiseDocumentChanged();
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        switch (e.Key)
        {
            case Key.Enter when _polyInProgress is not null:
                FinishPolyline();
                e.Handled = true;
                break;
            case Key.Escape when _polyInProgress is not null:
                ActiveLayer?.Objects.Remove(_polyInProgress);
                _polyInProgress = null;
                InvalidateVisual();
                e.Handled = true;
                break;
            case Key.Delete or Key.Back when SelectedObject is not null && Document is not null:
                DeleteSelected();
                e.Handled = true;
                break;
        }
    }

    /// <summary>Löscht das ausgewählte Objekt über die Historie.</summary>
    public void DeleteSelected()
    {
        if (SelectedObject is null || Document is null) return;
        var obj = SelectedObject;
        var layer = Document.Layers.FirstOrDefault(l => l.Objects.Contains(obj));
        if (layer is null) return;

        if (UndoStack is not null)
            UndoStack.Execute(new RemoveObjectCommand(layer, obj));
        else
            layer.Objects.Remove(obj);

        SelectedObject = null;
        RaiseDocumentChanged();
        InvalidateVisual();
    }

    private void FinishPolyline()
    {
        if (_polyInProgress is null) return;
        var layer = ActiveLayer;
        if (_polyInProgress.Points.Count < 2)
            layer?.Objects.Remove(_polyInProgress);
        else
            RegisterAdd(_polyInProgress, layer);
        _polyInProgress = null;
        InvalidateVisual();
    }

    private void RaiseDocumentChanged() => DocumentChanged?.Invoke(this, EventArgs.Empty);

    // ----- Resize-Handles -----

    private IEnumerable<(ResizeHandle Handle, Point Center)> HandlePositions()
    {
        if (SelectedObject is null) yield break;
        var (x, y, w, h) = SelectedObject.Bounds;
        var tl = ToScreen(x, y);
        var br = ToScreen(x + w, y + h);
        var cx = (tl.X + br.X) / 2;
        var cy = (tl.Y + br.Y) / 2;

        yield return (ResizeHandle.NW, tl);
        yield return (ResizeHandle.N, new Point(cx, tl.Y));
        yield return (ResizeHandle.NE, new Point(br.X, tl.Y));
        yield return (ResizeHandle.E, new Point(br.X, cy));
        yield return (ResizeHandle.SE, br);
        yield return (ResizeHandle.S, new Point(cx, br.Y));
        yield return (ResizeHandle.SW, new Point(tl.X, br.Y));
        yield return (ResizeHandle.W, new Point(tl.X, cy));
    }

    private ResizeHandle? HitHandle(Point screenPos)
    {
        foreach (var (handle, center) in HandlePositions())
            if (Math.Abs(screenPos.X - center.X) <= HandleSizePx && Math.Abs(screenPos.Y - center.Y) <= HandleSizePx)
                return handle;
        return null;
    }

    private void ApplyResize(ResizeHandle handle, Point mm)
    {
        var (x, y, w, h) = _resizeStartBounds;
        var dx = mm.X - _dragStartMm.X;
        var dy = mm.Y - _dragStartMm.Y;

        var movesLeft = handle is ResizeHandle.W or ResizeHandle.NW or ResizeHandle.SW;
        var movesRight = handle is ResizeHandle.E or ResizeHandle.NE or ResizeHandle.SE;
        var movesTop = handle is ResizeHandle.N or ResizeHandle.NW or ResizeHandle.NE;
        var movesBottom = handle is ResizeHandle.S or ResizeHandle.SW or ResizeHandle.SE;

        var newX = movesLeft ? x + dx : x;
        var newW = movesLeft ? w - dx : movesRight ? w + dx : w;
        var newY = movesTop ? y + dy : y;
        var newH = movesTop ? h - dy : movesBottom ? h + dy : h;

        if (newW < 0.1) { newW = 0.1; if (movesLeft) newX = x + w - 0.1; }
        if (newH < 0.1) { newH = 0.1; if (movesTop) newY = y + h - 0.1; }

        SelectedObject!.SetBounds(newX, newY, newW, newH);
    }

    private void UpdateCursor(Point screenPos)
    {
        if (Tool != CanvasTool.Select || SelectedObject is null)
        {
            Cursor = Cursor.Default;
            return;
        }
        Cursor = HitHandle(screenPos) switch
        {
            ResizeHandle.N or ResizeHandle.S => new Cursor(StandardCursorType.SizeNorthSouth),
            ResizeHandle.W or ResizeHandle.E => new Cursor(StandardCursorType.SizeWestEast),
            ResizeHandle.NW or ResizeHandle.SE => new Cursor(StandardCursorType.TopLeftCorner),
            ResizeHandle.NE or ResizeHandle.SW => new Cursor(StandardCursorType.TopRightCorner),
            _ => Cursor.Default,
        };
    }

    // ----- Rendering -----

    /// <summary>Rasterabstand in mm.</summary>
    private const double GridStepMm = 50;

    public override void Render(DrawingContext context)
    {
        context.FillRectangle(new SolidColorBrush(Color.FromRgb(30, 30, 34)), new Rect(Bounds.Size));
        if (Document is null) return;

        DrawGrid(context);
        DrawWorkArea(context);

        foreach (var layer in Document.Layers)
        {
            if (!layer.Visible) continue;
            var color = Color.TryParse(layer.ColorHex, out var c) ? c : Colors.OrangeRed;
            var pen = new Pen(new SolidColorBrush(color), 1.5);
            // Fill-/Raster-Layer zeigen ihre Formen halbtransparent gefüllt,
            // damit der Bearbeitungsmodus sofort erkennbar ist (ADR 0003, §5).
            var fill = layer.Mode.IsFilled()
                ? new SolidColorBrush(color, 0.28)
                : null;
            foreach (var obj in layer.Objects)
                DrawObject(context, obj, pen, fill);
        }

        // Vorschau-Segment der laufenden Polyline
        if (_polyInProgress is { Points.Count: > 0 })
        {
            var last = _polyInProgress.Points[^1];
            var previewPen = new Pen(Brushes.White, 1, dashStyle: DashStyle.Dot);
            context.DrawLine(previewPen, ToScreen(last.X, last.Y), ToScreen(_polyPreviewMm.X, _polyPreviewMm.Y));
        }

        DrawSelection(context);
    }

    /// <summary>
    /// Zeichnet das Raster über die gesamte sichtbare Fläche (unendliche
    /// Millimeterpapier-Ebene), ausgerichtet am mm-Nullpunkt.
    /// </summary>
    private void DrawGrid(DrawingContext context)
    {
        var step = GridStepMm;
        // Bei sehr kleinem Zoom das Raster ausdünnen, damit es nicht zumatscht.
        while (step * _zoom < 8) step *= 2;

        var gridPen = new Pen(new SolidColorBrush(Color.FromArgb(28, 255, 255, 255)));
        var axisPen = new Pen(new SolidColorBrush(Color.FromArgb(55, 255, 255, 255)));

        // Sichtbarer Bereich in mm
        var topLeftMm = ToMm(new Point(0, 0));
        var bottomRightMm = ToMm(new Point(Bounds.Width, Bounds.Height));

        var startX = Math.Floor(topLeftMm.X / step) * step;
        for (var x = startX; x <= bottomRightMm.X; x += step)
        {
            var sx = ToScreen(x, 0).X;
            context.DrawLine(Math.Abs(x) < 0.01 ? axisPen : gridPen,
                new Point(sx, 0), new Point(sx, Bounds.Height));
        }

        var startY = Math.Floor(topLeftMm.Y / step) * step;
        for (var y = startY; y <= bottomRightMm.Y; y += step)
        {
            var sy = ToScreen(0, y).Y;
            context.DrawLine(Math.Abs(y) < 0.01 ? axisPen : gridPen,
                new Point(0, sy), new Point(Bounds.Width, sy));
        }
    }

    /// <summary>
    /// Zeichnet den Laser-Arbeitsraum als farbig hervorgehobenes Rechteck
    /// (Größe der Maschinenfläche) mit markiertem Nullpunkt in der Ecke.
    /// </summary>
    private void DrawWorkArea(DrawingContext context)
    {
        var topLeft = ToScreen(0, 0);
        var rect = new Rect(topLeft, new Size(Document!.WidthMm * _zoom, Document.HeightMm * _zoom));

        // Farbige Arbeitsfläche, leicht hervorgehoben gegenüber dem Umfeld
        context.FillRectangle(new SolidColorBrush(Color.FromArgb(30, 90, 150, 220)), rect);
        context.DrawRectangle(new Pen(new SolidColorBrush(Color.FromRgb(90, 150, 220)), 1.5), rect);

        // Nullpunkt-Markierung in der oberen linken Ecke (Maschinen-Origin)
        var origin = ToScreen(0, 0);
        var markLen = 18.0;
        var originPen = new Pen(new SolidColorBrush(Color.FromRgb(240, 180, 60)), 2.5);
        context.DrawLine(originPen, origin, new Point(origin.X + markLen, origin.Y));
        context.DrawLine(originPen, origin, new Point(origin.X, origin.Y + markLen));
        context.DrawEllipse(new SolidColorBrush(Color.FromRgb(240, 180, 60)), null, origin, 3.5, 3.5);
    }

    private void DrawSelection(DrawingContext context)
    {
        if (SelectedObject is null) return;

        var (bx, by, bw, bh) = SelectedObject.Bounds;
        var tl = ToScreen(bx, by);
        var selRect = new Rect(tl, new Size(bw * _zoom, bh * _zoom));
        context.DrawRectangle(new Pen(Brushes.DeepSkyBlue, 1, dashStyle: DashStyle.Dash), selRect.Inflate(4));

        var handleBrush = Brushes.White;
        var handlePen = new Pen(Brushes.DeepSkyBlue, 1);
        foreach (var (_, center) in HandlePositions())
        {
            var r = new Rect(center.X - HandleSizePx / 2, center.Y - HandleSizePx / 2, HandleSizePx, HandleSizePx);
            context.DrawRectangle(handleBrush, handlePen, r);
        }
    }

    /// <summary>
    /// Zeichnet ein Objekt. <paramref name="fill"/> ist gesetzt, wenn der Layer
    /// im Fill-/Raster-Modus ist; füllbare Formen werden dann zusätzlich flächig
    /// hinterlegt (die Kontur bleibt immer erhalten).
    /// </summary>
    private void DrawObject(DrawingContext context, CanvasObject obj, Pen pen, IBrush? fill = null)
    {
        var objFill = obj.IsFillable ? fill : null;
        switch (obj)
        {
            case RectangleObject r:
                context.DrawRectangle(objFill, pen,
                    new Rect(ToScreen(r.X, r.Y), new Size(r.Width * _zoom, r.Height * _zoom)));
                break;
            case EllipseObject el:
                var center = ToScreen(el.X + el.Width / 2, el.Y + el.Height / 2);
                context.DrawEllipse(objFill, pen, center, el.Width / 2 * _zoom, el.Height / 2 * _zoom);
                break;
            case LineObject line:
                context.DrawLine(pen, ToScreen(line.X, line.Y), ToScreen(line.X2, line.Y2));
                break;
            case PolylineObject poly when poly.Points.Count >= 2:
                // Geschlossene, gefüllte Polygone als Geometrie zeichnen (Füllung + Kontur);
                // sonst Segment für Segment als Linienzug.
                if (objFill is not null && poly.Closed)
                {
                    var geo = new StreamGeometry();
                    using (var g = geo.Open())
                    {
                        g.BeginFigure(ToScreen(poly.Points[0].X, poly.Points[0].Y), isFilled: true);
                        for (var i = 1; i < poly.Points.Count; i++)
                            g.LineTo(ToScreen(poly.Points[i].X, poly.Points[i].Y));
                        g.EndFigure(isClosed: true);
                    }
                    context.DrawGeometry(objFill, pen, geo);
                    break;
                }
                for (var i = 1; i < poly.Points.Count; i++)
                    context.DrawLine(pen,
                        ToScreen(poly.Points[i - 1].X, poly.Points[i - 1].Y),
                        ToScreen(poly.Points[i].X, poly.Points[i].Y));
                if (poly.Closed)
                    context.DrawLine(pen,
                        ToScreen(poly.Points[^1].X, poly.Points[^1].Y),
                        ToScreen(poly.Points[0].X, poly.Points[0].Y));
                break;
        }
    }
}
