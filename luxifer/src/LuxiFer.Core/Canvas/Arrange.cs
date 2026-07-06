namespace LuxiFer.Core.Canvas;

/// <summary>Ausricht-Art für eine Objektmenge.</summary>
public enum AlignKind
{
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

/// <summary>Verteil-Art (gleiche Abstände) für eine Objektmenge.</summary>
public enum DistributeKind
{
    Horizontal,
    Vertical,
}

/// <summary>
/// Reine Anordnen-Berechnungen (Ausrichten/Verteilen) ohne UI-Bezug, im Core
/// und damit testbar (ADR 0004 §5). Ergebnis ist je Objekt ein Positions-Delta
/// in mm; das Anwenden/Undo übernimmt die Aufrufebene.
/// </summary>
public static class Arrange
{
    /// <summary>
    /// Deltas, um alle Objekte an der gemeinsamen Kante/Mitte auszurichten.
    /// Bezug ist die Bounding-Box der gesamten Menge. Reihenfolge entspricht
    /// <paramref name="objects"/>. Braucht ≥ 2 Objekte.
    /// </summary>
    public static IReadOnlyList<(double Dx, double Dy)> Align(
        IReadOnlyList<CanvasObject> objects, AlignKind kind)
    {
        var result = new (double, double)[objects.Count];
        if (objects.Count < 2)
            return result; // alle Deltas 0

        var (gx, gy, gw, gh) = GroupBounds(objects);
        for (var i = 0; i < objects.Count; i++)
        {
            var (bx, by, bw, bh) = objects[i].Bounds;
            double dx = 0, dy = 0;
            switch (kind)
            {
                case AlignKind.Left: dx = gx - bx; break;
                case AlignKind.Right: dx = gx + gw - (bx + bw); break;
                case AlignKind.HCenter: dx = gx + gw / 2 - (bx + bw / 2); break;
                case AlignKind.Top: dy = gy - by; break;
                case AlignKind.Bottom: dy = gy + gh - (by + bh); break;
                case AlignKind.VCenter: dy = gy + gh / 2 - (by + bh / 2); break;
            }
            result[i] = (dx, dy);
        }
        return result;
    }

    /// <summary>
    /// Deltas, um die Objekte mit gleichen Zwischenräumen zu verteilen. Die
    /// äußersten Objekte bleiben stehen; die inneren rücken auf gleiche Lücken.
    /// Braucht ≥ 3 Objekte.
    /// </summary>
    public static IReadOnlyList<(double Dx, double Dy)> Distribute(
        IReadOnlyList<CanvasObject> objects, DistributeKind kind)
    {
        var result = new (double, double)[objects.Count];
        if (objects.Count < 3)
            return result;

        // Nach Startkante der jeweiligen Achse sortieren (Index-Zuordnung merken).
        var order = new int[objects.Count];
        for (var i = 0; i < order.Length; i++) order[i] = i;

        double Start(int i)
        {
            var (bx, by, _, _) = objects[i].Bounds;
            return kind == DistributeKind.Horizontal ? bx : by;
        }

        Array.Sort(order, (a, b) => Start(a).CompareTo(Start(b)));

        var first = order[0];
        var last = order[^1];
        var span = Start(last) - Start(first);
        // Summe der Eigengrößen der inneren Objekte spielt beim
        // Startkanten-Verteilen keine Rolle: wir verteilen die Startkanten
        // gleichmäßig zwischen erstem und letztem.
        var step = span / (order.Length - 1);

        for (var k = 1; k < order.Length - 1; k++)
        {
            var idx = order[k];
            var targetStart = Start(first) + step * k;
            var delta = targetStart - Start(idx);
            result[idx] = kind == DistributeKind.Horizontal ? (delta, 0) : (0, delta);
        }
        return result;
    }

    /// <summary>Bounding-Box, die alle Objekte umschließt (mm).</summary>
    public static (double X, double Y, double W, double H) GroupBounds(
        IReadOnlyList<CanvasObject> objects)
    {
        double minX = double.MaxValue, minY = double.MaxValue;
        double maxX = double.MinValue, maxY = double.MinValue;
        foreach (var o in objects)
        {
            var (bx, by, bw, bh) = o.Bounds;
            minX = Math.Min(minX, bx);
            minY = Math.Min(minY, by);
            maxX = Math.Max(maxX, bx + bw);
            maxY = Math.Max(maxY, by + bh);
        }
        return (minX, minY, maxX - minX, maxY - minY);
    }
}
