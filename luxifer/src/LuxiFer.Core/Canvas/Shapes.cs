namespace LuxiFer.Core.Canvas;

/// <summary>Rechteck; X/Y ist die linke obere Ecke, Maße in mm.</summary>
public sealed class RectangleObject : CanvasObject
{
    public double Width { get; set; }
    public double Height { get; set; }

    public override (double X, double Y, double Width, double Height) Bounds => (X, Y, Width, Height);
    public override bool IsFillable => true;

    public override void SetBounds(double x, double y, double width, double height)
    {
        X = x;
        Y = y;
        Width = Math.Max(0.1, width);
        Height = Math.Max(0.1, height);
    }
}

/// <summary>Ellipse; X/Y ist die linke obere Ecke der Bounding-Box.</summary>
public sealed class EllipseObject : CanvasObject
{
    public double Width { get; set; }
    public double Height { get; set; }

    public override (double X, double Y, double Width, double Height) Bounds => (X, Y, Width, Height);
    public override bool IsFillable => true;

    public override void SetBounds(double x, double y, double width, double height)
    {
        X = x;
        Y = y;
        Width = Math.Max(0.1, width);
        Height = Math.Max(0.1, height);
    }
}

/// <summary>Linie von (X,Y) nach (X2,Y2).</summary>
public sealed class LineObject : CanvasObject
{
    public double X2 { get; set; }
    public double Y2 { get; set; }

    public override (double X, double Y, double Width, double Height) Bounds =>
        (Math.Min(X, X2), Math.Min(Y, Y2), Math.Abs(X2 - X), Math.Abs(Y2 - Y));

    public override void MoveBy(double dx, double dy)
    {
        base.MoveBy(dx, dy);
        X2 += dx;
        Y2 += dy;
    }

    public override void SetBounds(double x, double y, double width, double height)
    {
        var (bx, by, bw, bh) = Bounds;
        var sx = bw > 0 ? width / bw : 1;
        var sy = bh > 0 ? height / bh : 1;
        (X, Y) = (x + (X - bx) * sx, y + (Y - by) * sy);
        (X2, Y2) = (x + (X2 - bx) * sx, y + (Y2 - by) * sy);
    }
}

/// <summary>Offene (Polyline) oder geschlossene (Polygon) Punktfolge in mm.</summary>
public sealed class PolylineObject : CanvasObject
{
    public List<(double X, double Y)> Points { get; init; } = [];
    public bool Closed { get; set; }

    /// <summary>Nur geschlossene Polygone umschließen eine füllbare Fläche.</summary>
    public override bool IsFillable => Closed;

    public override (double X, double Y, double Width, double Height) Bounds
    {
        get
        {
            if (Points.Count == 0) return (X, Y, 0, 0);
            double minX = double.MaxValue, minY = double.MaxValue;
            double maxX = double.MinValue, maxY = double.MinValue;
            foreach (var (px, py) in Points)
            {
                minX = Math.Min(minX, px);
                minY = Math.Min(minY, py);
                maxX = Math.Max(maxX, px);
                maxY = Math.Max(maxY, py);
            }
            return (minX, minY, maxX - minX, maxY - minY);
        }
    }

    public override void MoveBy(double dx, double dy)
    {
        base.MoveBy(dx, dy);
        for (var i = 0; i < Points.Count; i++)
            Points[i] = (Points[i].X + dx, Points[i].Y + dy);
    }

    public override void SetBounds(double x, double y, double width, double height)
    {
        var (bx, by, bw, bh) = Bounds;
        var sx = bw > 0 ? width / bw : 1;
        var sy = bh > 0 ? height / bh : 1;
        for (var i = 0; i < Points.Count; i++)
            Points[i] = (x + (Points[i].X - bx) * sx, y + (Points[i].Y - by) * sy);
    }
}
