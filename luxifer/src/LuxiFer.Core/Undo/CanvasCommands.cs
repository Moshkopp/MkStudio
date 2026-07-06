using LuxiFer.Core.Canvas;

namespace LuxiFer.Core.Undo;

/// <summary>Fügt ein Objekt zu einem Layer hinzu.</summary>
public sealed class AddObjectCommand(Layer layer, CanvasObject obj) : IUndoableCommand
{
    public string Label => $"{ShapeName(obj)} hinzufügen";

    public void Do() => layer.Objects.Add(obj);

    public void Undo() => layer.Objects.Remove(obj);

    internal static string ShapeName(CanvasObject obj) => obj switch
    {
        RectangleObject => "Rechteck",
        EllipseObject => "Ellipse",
        LineObject => "Linie",
        PolylineObject { Closed: true } => "Polygon",
        PolylineObject => "Polyline",
        _ => "Objekt",
    };
}

/// <summary>
/// Bündelt mehrere Commands zu einem Undo-Schritt (z. B. Löschen mehrerer
/// ausgewählter Objekte). Undo läuft in umgekehrter Reihenfolge.
/// </summary>
public sealed class CompositeCommand(
    IReadOnlyList<IUndoableCommand> commands, string label) : IUndoableCommand
{
    public string Label => label;

    public void Do()
    {
        for (var i = 0; i < commands.Count; i++) commands[i].Do();
    }

    public void Undo()
    {
        for (var i = commands.Count - 1; i >= 0; i--) commands[i].Undo();
    }
}

/// <summary>Entfernt ein Objekt aus einem Layer und merkt sich Position für Undo.</summary>
public sealed class RemoveObjectCommand(Layer layer, CanvasObject obj) : IUndoableCommand
{
    private int _index = -1;

    public string Label => $"{AddObjectCommand.ShapeName(obj)} löschen";

    public void Do()
    {
        _index = layer.Objects.IndexOf(obj);
        if (_index >= 0) layer.Objects.RemoveAt(_index);
    }

    public void Undo()
    {
        if (_index < 0) return;
        _index = Math.Min(_index, layer.Objects.Count);
        layer.Objects.Insert(_index, obj);
    }
}

/// <summary>
/// Verschiebt ein Objekt um ein Delta. Delta-basiert, damit auch Linien und
/// Polylinien verlustfrei verschoben werden (nutzt MoveBy statt SetBounds).
/// Für einen interaktiv bereits vollzogenen Drag: das Gesamt-Delta übergeben.
/// </summary>
public sealed class MoveObjectCommand(CanvasObject obj, double dx, double dy) : IUndoableCommand
{
    public string Label => "Verschieben";

    public void Do() => obj.MoveBy(dx, dy);

    public void Undo() => obj.MoveBy(-dx, -dy);
}

/// <summary>
/// Ändert die Bounding-Box eines Objekts (Skalieren). Vorher-/Nachher-Zustand
/// werden als Bounds gespeichert, sodass ein interaktiv bereits vollzogener
/// Resize rückgängig gemacht werden kann.
/// </summary>
public sealed class ResizeObjectCommand(
    CanvasObject obj,
    (double X, double Y, double W, double H) before,
    (double X, double Y, double W, double H) after) : IUndoableCommand
{
    public string Label => "Skalieren";

    public void Do() => obj.SetBounds(after.X, after.Y, after.W, after.H);

    public void Undo() => obj.SetBounds(before.X, before.Y, before.W, before.H);
}

/// <summary>Dreht ein Objekt (Rotation in Grad); Vorher-/Nachher-Winkel.</summary>
public sealed class RotateObjectCommand(
    CanvasObject obj, double before, double after) : IUndoableCommand
{
    public string Label => "Drehen";

    public void Do() => obj.Rotation = after;

    public void Undo() => obj.Rotation = before;
}

/// <summary>
/// Verschiebt mehrere Objekte um je ein eigenes Delta (Ausrichten/Verteilen).
/// Eine Anordnen-Aktion ist damit ein einziger Undo-Schritt (ADR 0004 §5).
/// </summary>
public sealed class ArrangeObjectsCommand(
    IReadOnlyList<CanvasObject> objects,
    IReadOnlyList<(double Dx, double Dy)> deltas,
    string label) : IUndoableCommand
{
    public string Label => label;

    public void Do()
    {
        for (var i = 0; i < objects.Count; i++)
            objects[i].MoveBy(deltas[i].Dx, deltas[i].Dy);
    }

    public void Undo()
    {
        for (var i = 0; i < objects.Count; i++)
            objects[i].MoveBy(-deltas[i].Dx, -deltas[i].Dy);
    }
}
